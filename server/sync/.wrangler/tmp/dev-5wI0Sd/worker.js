var __defProp = Object.defineProperty;
var __name = (target, value) => __defProp(target, "name", { value, configurable: true });

// worker.mjs
import { DurableObject } from "cloudflare:workers";
var MAX_BODY = 8 * 1024 * 1024;
var CHUNK = 1024 * 1024;
var RETAIN_MS = 365 * 24 * 3600 * 1e3;
var ID_RE = /^\/v1\/([0-9a-f]{32})$/;
var text = /* @__PURE__ */ __name((status, body = "", headers = {}) => new Response(body, { status, headers: { "content-type": "text/plain; charset=utf-8", ...headers } }), "text");
function b64url(s) {
  if (!/^[A-Za-z0-9_-]+={0,2}$/.test(s)) return null;
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/").replace(/=+$/, ""));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}
__name(b64url, "b64url");
async function limited(binding, key) {
  if (!binding) return false;
  const { success } = await binding.limit({ key });
  return !success;
}
__name(limited, "limited");
async function readCapped(request) {
  const len = request.headers.get("content-length");
  if (len !== null && Number(len) > MAX_BODY) return null;
  if (!request.body) return new Uint8Array(0);
  const parts = [];
  let total = 0;
  const reader = request.body.getReader();
  for (; ; ) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > MAX_BODY) {
      await reader.cancel();
      return null;
    }
    parts.push(value);
  }
  const out = new Uint8Array(total);
  let off = 0;
  for (const p of parts) {
    out.set(p, off);
    off += p.byteLength;
  }
  return out;
}
__name(readCapped, "readCapped");
function etagNum(v) {
  const m = v && /^"(\d{1,15})"$/.exec(v.trim());
  return m ? Number(m[1]) : null;
}
__name(etagNum, "etagNum");
var worker_default = {
  async fetch(request, env) {
    const { pathname } = new URL(request.url);
    if (pathname === "/" && request.method === "GET") return text(200, "Gezik sync server\n");
    const m = ID_RE.exec(pathname);
    if (!m) return text(404, "Not found\n");
    const id = m[1];
    const method = request.method;
    if (method !== "GET" && method !== "PUT" && method !== "DELETE")
      return text(405, "Method not allowed\n", { allow: "GET, PUT, DELETE" });
    const ip = request.headers.get("cf-connecting-ip") ?? "local";
    if (await limited(env.RATE_IP, ip) || await limited(env.RATE_ID, id))
      return text(429, "Too many requests\n", { "retry-after": "60" });
    const auth = /^Bearer (\S+)$/.exec(request.headers.get("authorization") ?? "");
    const authBytes = auth && b64url(auth[1]);
    if (!authBytes || authBytes.length < 16) return text(403, "Forbidden\n");
    const authHash = new Uint8Array(await crypto.subtle.digest("SHA-256", authBytes));
    const box = env.BOX.get(env.BOX.idFromName(id));
    let r;
    if (method === "GET") r = await box.read(authHash);
    else if (method === "DELETE") r = await box.remove(authHash);
    else {
      const ifMatch = request.headers.get("if-match");
      const ifNone = request.headers.get("if-none-match");
      let expect;
      if (ifNone !== null && ifNone.trim() === "*") expect = 0;
      else if (ifMatch !== null) expect = etagNum(ifMatch) ?? -1;
      else return text(428, "Precondition required\n");
      const body = await readCapped(request);
      if (body === null) return text(413, "Too large\n");
      r = await box.write(authHash, expect, body);
    }
    const headers = r.version ? { etag: `"${r.version}"` } : {};
    if (r.body) return new Response(r.body, { status: r.status, headers: { "content-type": "application/octet-stream", ...headers } });
    return new Response(null, { status: r.status, headers });
  }
};
var Box = class extends DurableObject {
  static {
    __name(this, "Box");
  }
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.init();
  }
  init() {
    this.sql.exec(`CREATE TABLE IF NOT EXISTS meta (k INTEGER PRIMARY KEY CHECK (k = 0), auth BLOB NOT NULL, version INTEGER NOT NULL);
                   CREATE TABLE IF NOT EXISTS chunk (i INTEGER PRIMARY KEY, data BLOB NOT NULL);`);
  }
  meta() {
    return this.sql.exec("SELECT auth, version FROM meta").toArray()[0];
  }
  // 0 = no box, 403 = wrong auth, else the version.
  check(authHash) {
    const m = this.meta();
    if (!m) return 0;
    const stored = new Uint8Array(m.auth);
    if (stored.length !== authHash.length || !crypto.subtle.timingSafeEqual(stored, authHash)) return 403;
    return m.version;
  }
  async touch() {
    await this.ctx.storage.setAlarm(Date.now() + RETAIN_MS);
  }
  async read(authHash) {
    const v = this.check(authHash);
    if (v === 0) return { status: 404 };
    if (v === 403) return { status: 403 };
    await this.touch();
    const rows = this.sql.exec("SELECT data FROM chunk ORDER BY i").toArray();
    const total = rows.reduce((n, r) => n + r.data.byteLength, 0);
    const body = new Uint8Array(total);
    let off = 0;
    for (const r of rows) {
      body.set(new Uint8Array(r.data), off);
      off += r.data.byteLength;
    }
    return { status: 200, version: v, body };
  }
  async write(authHash, expect, body) {
    const v = this.check(authHash);
    if (v === 403) return { status: 403 };
    if (v !== expect) return { status: 412, version: v || void 0 };
    const next = v + 1;
    this.ctx.storage.transactionSync(() => {
      this.sql.exec("DELETE FROM chunk");
      for (let i = 0, off = 0; off < body.length; i++, off += CHUNK)
        this.sql.exec("INSERT INTO chunk (i, data) VALUES (?, ?)", i, body.slice(off, off + CHUNK).buffer);
      this.sql.exec("INSERT OR REPLACE INTO meta (k, auth, version) VALUES (0, ?, ?)", authHash.slice().buffer, next);
    });
    await this.touch();
    return { status: 200, version: next };
  }
  async remove(authHash) {
    const v = this.check(authHash);
    if (v === 0) return { status: 404 };
    if (v === 403) return { status: 403 };
    await this.wipe();
    return { status: 204 };
  }
  async wipe() {
    await this.ctx.storage.deleteAlarm();
    await this.ctx.storage.deleteAll();
    this.init();
  }
  async alarm() {
    await this.wipe();
  }
};

// C:/Users/teoma/AppData/Local/npm-cache/_npx/c943b712072b77c4/node_modules/wrangler/templates/middleware/middleware-ensure-req-body-drained.ts
var drainBody = /* @__PURE__ */ __name(async (request, env, _ctx, middlewareCtx) => {
  try {
    return await middlewareCtx.next(request, env);
  } finally {
    try {
      if (request.body !== null && !request.bodyUsed) {
        const reader = request.body.getReader();
        while (!(await reader.read()).done) {
        }
      }
    } catch (e) {
      console.error("Failed to drain the unused request body.", e);
    }
  }
}, "drainBody");
var middleware_ensure_req_body_drained_default = drainBody;

// C:/Users/teoma/AppData/Local/npm-cache/_npx/c943b712072b77c4/node_modules/wrangler/templates/middleware/middleware-miniflare3-json-error.ts
function reduceError(e) {
  return {
    name: e?.name,
    message: e?.message ?? String(e),
    stack: e?.stack,
    cause: e?.cause === void 0 ? void 0 : reduceError(e.cause)
  };
}
__name(reduceError, "reduceError");
var jsonError = /* @__PURE__ */ __name(async (request, env, _ctx, middlewareCtx) => {
  try {
    return await middlewareCtx.next(request, env);
  } catch (e) {
    const error = reduceError(e);
    const body = JSON.stringify(error);
    const headers = {
      "Content-Type": "application/json",
      "MF-Experimental-Error-Stack": "true"
    };
    const encoded = encodeURIComponent(body);
    if (encoded.length <= 8192) {
      headers["MF-Experimental-Error-Stack-Payload"] = encoded;
    }
    return new Response(body, { status: 500, headers });
  }
}, "jsonError");
var middleware_miniflare3_json_error_default = jsonError;

// .wrangler/tmp/bundle-Sl0vCY/middleware-insertion-facade.js
var __INTERNAL_WRANGLER_MIDDLEWARE__ = [
  middleware_ensure_req_body_drained_default,
  middleware_miniflare3_json_error_default
];
var middleware_insertion_facade_default = worker_default;

// C:/Users/teoma/AppData/Local/npm-cache/_npx/c943b712072b77c4/node_modules/wrangler/templates/middleware/common.ts
var __facade_middleware__ = [];
function __facade_register__(...args) {
  __facade_middleware__.push(...args.flat());
}
__name(__facade_register__, "__facade_register__");
function __facade_invokeChain__(request, env, ctx, dispatch, middlewareChain) {
  const [head, ...tail] = middlewareChain;
  const middlewareCtx = {
    dispatch,
    next(newRequest, newEnv) {
      return __facade_invokeChain__(newRequest, newEnv, ctx, dispatch, tail);
    }
  };
  return head(request, env, ctx, middlewareCtx);
}
__name(__facade_invokeChain__, "__facade_invokeChain__");
function __facade_invoke__(request, env, ctx, dispatch, finalMiddleware) {
  return __facade_invokeChain__(request, env, ctx, dispatch, [
    ...__facade_middleware__,
    finalMiddleware
  ]);
}
__name(__facade_invoke__, "__facade_invoke__");

// .wrangler/tmp/bundle-Sl0vCY/middleware-loader.entry.ts
var __Facade_ScheduledController__ = class ___Facade_ScheduledController__ {
  constructor(scheduledTime, cron, noRetry) {
    this.scheduledTime = scheduledTime;
    this.cron = cron;
    this.#noRetry = noRetry;
  }
  scheduledTime;
  cron;
  static {
    __name(this, "__Facade_ScheduledController__");
  }
  #noRetry;
  noRetry() {
    if (!(this instanceof ___Facade_ScheduledController__)) {
      throw new TypeError("Illegal invocation");
    }
    this.#noRetry();
  }
};
function wrapExportedHandler(worker) {
  if (__INTERNAL_WRANGLER_MIDDLEWARE__ === void 0 || __INTERNAL_WRANGLER_MIDDLEWARE__.length === 0) {
    return worker;
  }
  for (const middleware of __INTERNAL_WRANGLER_MIDDLEWARE__) {
    __facade_register__(middleware);
  }
  const fetchDispatcher = /* @__PURE__ */ __name(function(request, env, ctx) {
    if (worker.fetch === void 0) {
      throw new Error("Handler does not export a fetch() function.");
    }
    return worker.fetch(request, env, ctx);
  }, "fetchDispatcher");
  return {
    ...worker,
    fetch(request, env, ctx) {
      const dispatcher = /* @__PURE__ */ __name(function(type, init) {
        if (type === "scheduled" && worker.scheduled !== void 0) {
          const controller = new __Facade_ScheduledController__(
            Date.now(),
            init.cron ?? "",
            () => {
            }
          );
          return worker.scheduled(controller, env, ctx);
        }
      }, "dispatcher");
      return __facade_invoke__(request, env, ctx, dispatcher, fetchDispatcher);
    }
  };
}
__name(wrapExportedHandler, "wrapExportedHandler");
function wrapWorkerEntrypoint(klass) {
  if (__INTERNAL_WRANGLER_MIDDLEWARE__ === void 0 || __INTERNAL_WRANGLER_MIDDLEWARE__.length === 0) {
    return klass;
  }
  for (const middleware of __INTERNAL_WRANGLER_MIDDLEWARE__) {
    __facade_register__(middleware);
  }
  return class extends klass {
    #fetchDispatcher = /* @__PURE__ */ __name((request, env, ctx) => {
      this.env = env;
      this.ctx = ctx;
      if (super.fetch === void 0) {
        throw new Error("Entrypoint class does not define a fetch() function.");
      }
      return super.fetch(request);
    }, "#fetchDispatcher");
    #dispatcher = /* @__PURE__ */ __name((type, init) => {
      if (type === "scheduled" && super.scheduled !== void 0) {
        const controller = new __Facade_ScheduledController__(
          Date.now(),
          init.cron ?? "",
          () => {
          }
        );
        return super.scheduled(controller);
      }
    }, "#dispatcher");
    fetch(request) {
      return __facade_invoke__(
        request,
        this.env,
        this.ctx,
        this.#dispatcher,
        this.#fetchDispatcher
      );
    }
  };
}
__name(wrapWorkerEntrypoint, "wrapWorkerEntrypoint");
var WRAPPED_ENTRY;
if (typeof middleware_insertion_facade_default === "object") {
  WRAPPED_ENTRY = wrapExportedHandler(middleware_insertion_facade_default);
} else if (typeof middleware_insertion_facade_default === "function") {
  WRAPPED_ENTRY = wrapWorkerEntrypoint(middleware_insertion_facade_default);
}
var middleware_loader_entry_default = WRAPPED_ENTRY;
export {
  Box,
  __INTERNAL_WRANGLER_MIDDLEWARE__,
  middleware_loader_entry_default as default
};
//# sourceMappingURL=worker.js.map
