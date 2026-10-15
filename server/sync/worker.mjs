// Gezik settings sync server: one encrypted blob per id, versioned, in a
// SQLite-backed Durable Object. The server never sees keys or plaintext.
// Never log request bodies or Authorization headers.
import { DurableObject } from "cloudflare:workers";

const MAX_BODY = 8 * 1024 * 1024;
const CHUNK = 1024 * 1024; // DO SQLite rows/blobs are capped at 2 MB
const RETAIN_MS = 365 * 24 * 3600 * 1000;
const ID_RE = /^\/v1\/([0-9a-f]{32})$/;

const text = (status, body = "", headers = {}) =>
  new Response(body, { status, headers: { "content-type": "text/plain; charset=utf-8", ...headers } });

function b64url(s) {
  if (!/^[A-Za-z0-9_-]+={0,2}$/.test(s)) return null;
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/").replace(/=+$/, ""));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

async function limited(binding, key) {
  if (!binding) return false; // no binding in some local setups
  const { success } = await binding.limit({ key });
  return !success;
}

// Reads the body, stopping as soon as it exceeds MAX_BODY. Returns null if too large.
async function readCapped(request) {
  const len = request.headers.get("content-length");
  if (len !== null && Number(len) > MAX_BODY) return null;
  if (!request.body) return new Uint8Array(0);
  const parts = [];
  let total = 0;
  const reader = request.body.getReader();
  for (;;) {
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

// Parses `"<n>"` into n, else null.
function etagNum(v) {
  const m = v && /^"(\d{1,15})"$/.exec(v.trim());
  return m ? Number(m[1]) : null;
}

export default {
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
    if ((await limited(env.RATE_IP, ip)) || (await limited(env.RATE_ID, id)))
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
      let expect; // 0 = must not exist, n = must be at version n
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
  },
};

export class Box extends DurableObject {
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
    if (v !== expect) return { status: 412, version: v || undefined };
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
    this.init(); // this instance may get more requests
  }

  async alarm() {
    await this.wipe();
  }
}
