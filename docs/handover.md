# Handover

**Steps 1 to 5 of `docs/plan.md` are done, and Step 6 is half done: tileboard is deployed with the demo app (2026-11-24).**

The owner was away from Step 4 on and said to go on without his decisions. What was decided without him is marked **(mine)** in the decisions log of `docs/plan.md`, with a short list in `.private/NOTES.md`.

## Open, in order
1. **Owner: look at the decisions marked (mine)** and say what to change.
2. **Owner: put the layout in the nightly backup.** The patch is written out in `.private/NOTES.md`; it goes into another project's production script, so it was not done unattended.
3. **First real app: nuc-ops** (owner's pick). In a session in that app's repo, give the agent the hand-over text from Settings → Connect an app (it points at the dashboard's `/connect.md`). The agent builds `/widgets`, checks it with `POST /api/check`, and registers the app with `POST /api/apps`. No file editing. That completes "v1 done" in the plan.
4. Small things listed under "Not done" below.

## What exists
- `SPEC.md` + `schema/`: widget contract v2.
- Demo app (made-up data): three sources, every field type, 12 presets.
- Dashboard server: config, hub (catalogue + data cache, background retry for apps that are down), layout store, browser API.
- View mode: five views, auto-refresh, stale and error states, light and dark, phone layout.
- Edit mode: drag, resize, keyboard, add (suggested or built in the picker, with a live preview), change, remove; Done saves, Cancel undoes; a save conflict asks which layout to keep.
- Connecting apps: Settings → Connect an app, `/connect.md` (the kit), `POST /api/check`, `POST /api/apps`, `DELETE /api/apps/<name>` (the last three need `manage_apps = true`; on in `--demo` and on the home server).
- `README.md` with pictures of the demo (`docs/images/`, retaken with `node tools/pictures.mjs` against a fresh `--demo`).
- Deployed on the home server, VPN address only: `bash .private/deploy.sh` redeploys.
- 122 Rust tests (one of them runs the 48 frontend tests), `cargo clippy --all-targets -- -D warnings` clean.

## How to check frontend work
1. `cargo run -- --demo --web-dir web` (changes show on reload).
2. `node tools/look.mjs` → pictures in `shots/` (git-ignored), sparkline readout, overflow check, page errors.
3. `node tools/try-edit.mjs` against a **freshly started** `--demo` → every edit-mode action, checked against what got saved.
4. **Open the pictures.** Enlarge with `clip` before judging thin lines.
5. For failure states: run a second dashboard from a config with the demo app plus an app on a closed port, save a layout with `PUT /api/layout`, then stop the demo app.

## Not done (known, small)
- Dragging near the edge of the window does not scroll the page.
- Other tiles jump to their new place while one is dragged (no slide).
- The picker gives every field of one source the same params (the layout file allows different ones).
- **HTTPS apps:** not callable yet (no TLS crate). Third-party gate question when a real app needs it.
- **Host check:** the dashboard has no login and does not check the `Host` header. Fine on the VPN; a host allow-list would close DNS-rebinding if it is ever exposed wider.
- **Sparkline points have no times:** the contract sends evenly spaced values only, so the readout says "3 before latest". A time per point would be a contract change.

## Standing rules
- Repo is public: read the "This repo is PUBLIC" section of `CLAUDE.md` before writing anything.
- Every crate / JS library goes through the third-party gate in `CLAUDE.md` first.
