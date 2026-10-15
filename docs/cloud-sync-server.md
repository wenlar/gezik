# Cloud sync server

Gezik's cloud sync (`[cloud-sync]` in settings) keeps one encrypted bundle per
sync code on a small server. The default server is `https://sync.wenlar.com`.
Its source is in `server/sync/`: a Cloudflare Worker (`worker.mjs`, no
dependencies) with one SQLite-backed Durable Object per sync box.

## What the server stores and sees

The app derives three values from your sync code: an `id`, an `auth` secret and
an encryption `key`. Only `id` and `auth` are sent; the key never leaves your
devices, and the bundle is encrypted (XChaCha20-Poly1305) before upload.

Per box the server stores:

- the encrypted bundle (opaque bytes),
- a version number,
- the SHA-256 of `auth` (not `auth` itself).

The server can see the box id, the bundle's size, when it is accessed and from
which IP address. It cannot see your settings, file names, themes, templates or
your sync code. The Worker does not log request bodies or the `Authorization`
header.

## API

All requests carry `Authorization: Bearer <auth, base64url>`; `<id>` is 32
lowercase hex characters.

| Request | Result |
|---|---|
| `GET /v1/<id>` | `200` + bundle + `ETag: "<n>"`, or `404` |
| `PUT /v1/<id>` + `If-None-Match: *` | create; `200` + `ETag: "1"`, `412` if it exists |
| `PUT /v1/<id>` + `If-Match: "<n>"` | replace; `200` + new `ETag`, `412` if the version is not `n` |
| `DELETE /v1/<id>` | `204`, removes everything |

The first `PUT` sets the box's auth; any later request with another auth gets
`403`. A `PUT` without `If-Match`/`If-None-Match` gets `428`.

## Limits

- Bundle size: at most 8 MiB (`413`).
- Rate limits (Workers Rate Limiting bindings, which only allow 10 or 60 second
  windows): 30 requests per minute per box id (`RATE_ID`) and 60 requests per
  minute per IP address (`RATE_IP`). Over the limit: `429` with
  `Retry-After: 60`. The limits are counted per Cloudflare location, so they
  are approximate.

## Retention

Every read or write sets the box to expire 365 days later. A box nobody touches
for a year is deleted entirely; the app then offers to start syncing again from
the settings it still has locally. Losing the sync code means the data cannot be
recovered, by design.

## Running your own server

You need a Cloudflare account (the free plan is enough) and Node.js.

1. In `server/sync/wrangler.toml`, change or remove the `routes` line
   (`sync.wenlar.com`) to a domain on your account, or set `workers_dev = true`
   to use the `*.workers.dev` address. The rate-limit `namespace_id`s are just
   numbers unique in your account; nothing has to be created for them.
2. From `server/sync/`: `npx wrangler@4 login`, then `npx wrangler@4 deploy`.
3. In Gezik, point the app at it in `settings.toml`:

   ```toml
   [cloud-sync]
   server = "https://sync.example.com"
   ```

Every device that shares a sync code must use the same server.

## Testing

```sh
cd server/sync
npx wrangler@4 dev            # local server on http://127.0.0.1:8787
node --test test.mjs          # in another terminal; GEZIK_SYNC_URL picks another server
```
