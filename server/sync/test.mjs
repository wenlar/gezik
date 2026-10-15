// Run against `npx wrangler dev` (or any deployment):
//   GEZIK_SYNC_URL=http://127.0.0.1:8787 node --test server/sync/test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { randomBytes } from "node:crypto";

const BASE = (process.env.GEZIK_SYNC_URL ?? "http://127.0.0.1:8787").replace(/\/$/, "");
const MiB = 1024 * 1024;

const newBox = () => ({ url: `${BASE}/v1/${randomBytes(16).toString("hex")}`, auth: randomBytes(32).toString("base64url") });
const req = (box, method, { auth = box.auth, headers = {}, body } = {}) =>
  fetch(box.url, { method, body, headers: { authorization: `Bearer ${auth}`, ...headers } });
const create = (box, body) => req(box, "PUT", { headers: { "if-none-match": "*" }, body });

test("root and unknown paths", async () => {
  const r = await fetch(`${BASE}/`);
  assert.equal(r.status, 200);
  assert.match(await r.text(), /Gezik sync server/);
  assert.equal((await fetch(`${BASE}/v1/ABC`)).status, 404);
  assert.equal((await fetch(`${BASE}/v1/${"0".repeat(31)}`)).status, 404);
  assert.equal((await fetch(`${BASE}/other`)).status, 404);
});

test("new id is 404", async () => {
  assert.equal((await req(newBox(), "GET")).status, 404);
});

test("create, then GET returns body and ETag", async () => {
  const box = newBox();
  const put = await create(box, "hello");
  assert.equal(put.status, 200);
  assert.equal(put.headers.get("etag"), '"1"');
  const get = await req(box, "GET");
  assert.equal(get.status, 200);
  assert.equal(get.headers.get("etag"), '"1"');
  assert.equal(await get.text(), "hello");
});

test("If-None-Match on existing box and stale If-Match are 412", async () => {
  const box = newBox();
  await create(box, "a");
  assert.equal((await create(box, "b")).status, 412);
  const ok = await req(box, "PUT", { headers: { "if-match": '"1"' }, body: "b" });
  assert.equal(ok.status, 200);
  assert.equal(ok.headers.get("etag"), '"2"');
  const stale = await req(box, "PUT", { headers: { "if-match": '"1"' }, body: "c" });
  assert.equal(stale.status, 412);
  assert.equal(await (await req(box, "GET")).text(), "b");
});

test("wrong auth is 403", async () => {
  const box = newBox();
  await create(box, "secret");
  const auth = randomBytes(32).toString("base64url");
  assert.equal((await req(box, "GET", { auth })).status, 403);
  assert.equal((await req(box, "PUT", { auth, headers: { "if-match": '"1"' }, body: "x" })).status, 403);
  assert.equal((await req(box, "DELETE", { auth })).status, 403);
  assert.equal((await fetch(box.url)).status, 403); // no Authorization at all
});

test("missing precondition is 428", async () => {
  assert.equal((await req(newBox(), "PUT", { body: "x" })).status, 428);
});

test("8 MiB + 1 is 413, 8 MiB is fine", async () => {
  const box = newBox();
  assert.equal((await create(box, new Uint8Array(8 * MiB + 1))).status, 413);
  assert.equal((await req(box, "GET")).status, 404);
  assert.equal((await create(box, new Uint8Array(8 * MiB))).status, 200);
});

test("3 MiB body round-trips byte-exact", async () => {
  const box = newBox();
  const data = randomBytes(3 * MiB + 123);
  assert.equal((await create(box, data)).status, 200);
  const got = Buffer.from(await (await req(box, "GET")).arrayBuffer());
  assert.ok(got.equals(data));
  // Shrinking drops the old rows.
  assert.equal((await req(box, "PUT", { headers: { "if-match": '"1"' }, body: "small" })).status, 200);
  assert.equal(await (await req(box, "GET")).text(), "small");
});

test("DELETE is 204, then 404, and the id can start over", async () => {
  const box = newBox();
  await create(box, "x");
  assert.equal((await req(box, "DELETE")).status, 204);
  assert.equal((await req(box, "GET")).status, 404);
  const again = await create(box, "y");
  assert.equal(again.status, 200);
  assert.equal(again.headers.get("etag"), '"1"');
});
