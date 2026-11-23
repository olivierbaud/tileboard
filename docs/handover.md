# Handover

**Steps 1 to 3 of `docs/plan.md` are done (2026-11-24). Next: Step 4, edit mode.**

## What exists
- `SPEC.md` + `schema/`: widget contract v2.
- Demo app (made-up data): three sources, every field type, 12 presets.
- Dashboard server: config, hub (catalogue + data cache, background retry for apps that are down), layout store, browser API.
- View mode: `cargo run -- --demo` → `http://127.0.0.1:4700`. Eleven starter tiles, five views, auto-refresh, stale and error states, light and dark, a first phone layout.
- 121 Rust tests (one of them runs the 30 frontend tests), `cargo clippy --all-targets -- -D warnings` clean.

## How to check frontend work
1. `cargo run -- --demo --web-dir web` (changes show on reload).
2. `node tools/look.mjs` → pictures in `shots/` (git-ignored), sparkline readout, overflow check, page errors.
3. **Open the pictures.** Enlarge with `clip` before judging thin lines.
4. For failure states: run a second dashboard from a config with the demo app plus an app on a closed port, save a layout with `PUT /api/layout`, then stop the demo app.

## Step 4 starts with
- **Third-party gate first** for drag and resize: gridstack.js (vendored, pinned, with its licence) or our own code. Name it, publisher, maturity, how it updates, what it can reach; then wait for the owner's go. Our own code is realistic here: the grid is 12 fixed columns and fixed row height, and tiles are already placed with `--x`, `--y`, `--w`, `--h` custom properties.
- **The page's Content-Security-Policy** forbids inline scripts and `style="..."` attributes; styles set from JavaScript are fine. A vendored library that injects a `<style>` tag would be blocked.
- **Saving:** `PUT /api/layout` with the whole layout and the `rev` it started from → `409` if someone saved first (reload and say so), `400` with `problems` if not valid.
- **The picker** (app → source → params → fields → view) has everything in `state.apps`: catalogues with field types, params with options and defaults, and `presets` for one-click adding. `pickView` in `web/model.js` already knows which views fit which fields.
- **A live preview** of a tile not yet saved: `POST /api/data` takes any source of a configured app.
- **Tile ids** are made by the page: 1 to 64 letters, digits, `_`, `-`, unique.
- **Empty board:** its text says edit mode is not built yet (`drawEmptyBoard` in `web/app.js`). Replace it with the way in.
- **One-row and one-column tiles** already degrade well; edit mode should not allow less than 1 x 1.

## Later, not forgotten
- **HTTPS apps:** not callable yet (no TLS crate). Gate question when a real app needs it.
- **Host check:** the dashboard has no login and does not check the `Host` header. Fine on Tailscale; a host allow-list would close DNS-rebinding if it is ever exposed wider.
- **Phone layout polish, README with pictures, privacy review:** Step 5. `tools/look.mjs` already takes the pictures.
- **Sparkline points have no times:** the contract sends evenly spaced values only, so the readout says "3 before latest". A time per point would be a contract change.

## Standing rules
- Repo is public: read the "This repo is PUBLIC" section of `CLAUDE.md` before writing anything.
- Every crate / JS library goes through the third-party gate in `CLAUDE.md` first.
