# Handover

**Steps 1 and 2 of `docs/plan.md` are done (2026-11-24). Next: Step 3, view mode.**

## What exists
- `SPEC.md` + `schema/`: widget contract v2.
- Demo app (made-up data): three sources, every field type, 12 presets.
- Dashboard server: `cargo run -- --demo` → `http://127.0.0.1:4700` (demo app on `:4701`). Config format in `config.example.toml`.
- A placeholder page (`web/`) that lists the apps. Step 3 replaces it.
- 109 tests, `cargo clippy --all-targets -- -D warnings` clean.

## What the frontend gets (all JSON)
- `GET /api/apps` → `{apps: [{name, ok, error, checked_at, catalogue}]}`. `catalogue` is the app's contract catalogue (labels, units, types, presets); it stays available when the app goes down.
- `GET /api/layout` → `{rev, tiles: [{id, label, view, x, y, w, h, fields: [{app, source, params?, field}]}]}`. With `--demo` there are 11 starter tiles covering all five views.
- `POST /api/data` with `{sources: [{app, source, params?}]}` → `{now, layout_rev, sources: [{app, source, params, ok, error, refresh_s, fetched_at, updated_at, link, values}]}`, same order as asked.
  - `values[field]` = `{value, status?, series?, error?}`. `status` is already worked out (thresholds included): the frontend never needs `warn` / `crit` for colour, only to draw lines on a sparkline.
  - `ok: false` with `values` present = old values after a failed call (stale). `ok: false` with empty `values` = never worked.
  - `now` is the server's clock: use it, not the browser's, to tell how old `updated_at` / `fetched_at` are.
  - `layout_rev` changing means the layout was saved elsewhere: load it again.
- `PUT /api/layout` with the whole layout and the `rev` it started from → the saved layout (`rev` + 1), `409` if someone saved first, `400` with `problems` if it is not valid.

## Step 3 notes
- **Ask per app, not all at once:** one `POST /api/data` waits for its slowest source (up to 5 s for an app that hangs). One request per app keeps the other apps' tiles fresh.
- **Ask as often as the fastest `refresh_s` on screen** (the server never calls an app more often than its `refresh_s`, so asking early is free).
- **Everything from an app is text:** `textContent`, never `innerHTML`. Only follow `link` as given by the server.
- **The page's Content-Security-Policy** forbids inline `<script>` and `style="..."` attributes. Styles set from JavaScript are fine.
- **Number formatting and unit placement** are the frontend's job (currency before the number, `%` after).
- **Stale rule to decide:** when is data "old"? The server gives the facts (`now`, `updated_at`, `fetched_at`, `refresh_s`, `ok`).
- A new frontend file needs an entry in `src/web.rs`. Work with `--web-dir web` to skip rebuilds.

## Later, not forgotten
- **HTTPS apps:** not callable yet (no TLS crate). Gate question when a real app needs it.
- **Host check:** the dashboard has no login and does not check the `Host` header. Fine on Tailscale; a host allow-list would close DNS-rebinding if it is ever exposed wider.
- **Demo app token:** the demo app has no token option; the bearer path is tested with the fake app in `tests/dashboard.rs`.

## Standing rules
- Repo is public: read the "This repo is PUBLIC" section of `CLAUDE.md` before writing anything.
- Every crate / JS library goes through the third-party gate in `CLAUDE.md` first.
