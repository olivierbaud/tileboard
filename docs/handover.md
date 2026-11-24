# Handover

**All seven steps of `docs/plan.md` are built and tileboard runs on the home server (2026-11-24). One thing is left for "v1 done": connect the first real app.**

## Resume prompt
Read `CLAUDE.md`, this file, `docs/plan.md` (decisions log), then `.private/NOTES.md` (addresses, deploy, which app is first).
tileboard is deployed with the demo app only. Next: the first real app is connected from a session in **that app's** repo, using the hand-over text in Settings → Connect an app.
Before changing anything, ask the owner which of the decisions marked **(mine)** in `docs/plan.md` he wants changed.

## The owner's decisions this session (his words)
- Crates for the server: "Approve all 4 (Recommended)" (tokio, axum, serde, serde_json).
- HTTP client and config: "hyper-util, HTTP only (Recommended)" and "TOML via the toml crate (Recommended)".
- While away: "Go without my consent now, I am not available, so do whatever you can without my decisions". The third-party gate stayed in force; no crate or library was added after that.
- Connecting apps: "we need to find a way to connect an app. something ion the settings. that could give any agent or developper what they need to connect." He also named the app to connect first (private notes).
- The backup: "i dont understand your backup thing and what it has to do with vertigo triage", then, once explained, "1" (do it now).

## Shipped
| Commit | What |
|---|---|
| `c24f466` | Step 1: contract v2 (`SPEC.md`, schemas), demo app |
| `677f178` | Step 2: dashboard server (config, hub, layout store, API) |
| `1343200` | Step 3: view mode; hub answers at once for an app that is down |
| `6597f03` | Step 4: edit mode (own drag and resize, picker) |
| `dc19f0b` | Step 5: README, pictures, phone layout, privacy review |
| `ddd09d1` | Step 6: deployed with the demo app; service example in the README |
| `54caba2` | Step 7: Settings → Connect an app, `/connect.md`, checker, add and remove apps |

The home server's nightly backup now includes the layout, the apps added from the settings page and the config (done in the backup script's own project, with the owner's go).

## Open, as choices (default first)
1. **Connect the first real app.**
   1. (default) In a session in that app's repo, paste the hand-over text from Settings → Connect an app. The agent builds `/widgets`, checks it with `POST /api/check`, registers it with `POST /api/apps`. No file editing, no restart.
   2. Write the app into the server's `config.toml` by hand and restart.
2. **Decisions marked (mine)** in `docs/plan.md` (own drag and resize, tiles always packed upward, one tile may mix sources and apps, the look, the phone layout, app management on for the home server).
   1. (default) Keep them.
   2. Say which to change.
3. **A name in the public history.** Commit `54caba2` named one of the owner's apps in two docs. The current files no longer do; the old commit still does. It is a name only: no address, no secret.
   1. (default) Leave the history as it is.
   2. Rewrite the public history to remove it (a force-push: the owner decides).

## What exists
- `SPEC.md` + `schema/`: widget contract v2.
- Demo app (made-up data): three sources, every field type, 12 presets.
- Dashboard server: config, hub (catalogue + data cache, background retry for apps that are down, apps can be added while running), layout store, browser API.
- View mode: five views, auto-refresh, stale and error states, light and dark, phone layout.
- Edit mode: drag, resize, keyboard, add (suggested or built in the picker, with a live preview), change, remove; Done saves, Cancel undoes; a save conflict asks which layout to keep.
- Connecting apps: Settings → Connect an app, `/connect.md` (the kit), `POST /api/check`, `POST /api/apps`, `DELETE /api/apps/<name>` (the last three need `manage_apps = true`; on in `--demo` and on the home server).
- `README.md` with pictures of the demo (`docs/images/`, retaken with `node tools/pictures.mjs` against a fresh `--demo`).
- Deployed on the home server, VPN address only: `bash .private/deploy.sh` redeploys.
- 131 Rust tests (one of them runs the 48 frontend tests), `cargo clippy --all-targets -- -D warnings` clean.

## How to check frontend work
1. `cargo run -- --demo --web-dir web` (changes show on reload).
2. `node tools/look.mjs` → pictures in `shots/` (git-ignored), sparkline readout, overflow check, page errors.
3. `node tools/try-edit.mjs` against a **freshly started** `--demo` → every edit-mode action, checked against what got saved.
4. **Open the pictures.** Enlarge with `clip` before judging thin lines.
5. For failure states: run a second dashboard from a config with the demo app plus an app on a closed port, save a layout with `PUT /api/layout`, then stop the demo app.

## Dead ends (do not repeat)
- **`cargo metadata --offline`** for a dependency dry run fails: it wants every platform's crates. Use `cargo tree --offline` in a scratch copy.
- **ssh one-liners from PowerShell** get their quotes mangled. Pipe a script through Git Bash: `ssh host 'bash -s' <<'REMOTE'`.
- **A piped remote script stops early** if a command in it reads stdin (the server's backup script does). Give that command `< /dev/null`.
- **A commit message that names the config key for an app's address** trips the owner's guard hook. Write "address".
- **`node --test <folder>`** finds nothing on Node 22. Use a quoted glob.
- **Waiting for the app list before drawing the board** made every page load wait on an app that is down. The board now draws from the layout alone.
- **Real app names in public docs.** They belong in `.private/NOTES.md`; see open item 3.

## Not done (known, small)
- Dragging near the edge of the window does not scroll the page.
- Other tiles jump to their new place while one is dragged (no slide).
- The picker gives every field of one source the same params (the layout file allows different ones).
- **HTTPS apps:** not callable yet (no TLS crate). Third-party gate question when a real app needs it.
- **Host check:** the dashboard has no login and does not check the `Host` header. Fine on the VPN; a host allow-list would close DNS-rebinding if it is ever exposed wider.
- **Sparkline points have no times:** the contract sends evenly spaced values only, so the readout says "3 before latest". A time per point would be a contract change.

## Standing rules
- Repo is public: read the "This repo is PUBLIC" section of `CLAUDE.md` before writing anything. Real app names count as private.
- Every crate / JS library goes through the third-party gate in `CLAUDE.md` first.
