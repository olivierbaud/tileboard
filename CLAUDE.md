# tileboard

A modular dashboard: apps describe their own data through two read-only endpoints (`/widgets`), and you build tiles from any mix of that data, then drag and resize them in edit mode. **On resume, read `docs/handover.md` first**, then `docs/plan.md`. **Then read `.private/NOTES.md` if it exists** (git-ignored: owner's paths, deploy target, app addresses, wrap-up duties).

## This repo is PUBLIC (MIT)
- **Nothing personal is ever committed:** no IPs, hostnames, Tailscale names, tokens, real app addresses, real layouts, real data, local paths or emails. Those live in `.private/` or a git-ignored config file. Fixtures, examples and screenshots use made-up data only (the demo app).
- No employer, customer or site names. No security / access-control integrations in this repo.
- Before every commit: `git diff --cached` and look for anything that wouldn't be safe on the public internet. GitHub secret scanning + push protection are on; they are a backstop, not the check.
- Commits use the GitHub noreply email (repo-local git config). Don't change it.

## Owner's choices (keep unless Olivier changes them)
- **Unlimited tiles, any mix of data:** "if you want just a widget with just 1 data, and another widget with another data, from the same app, then another with 2 datas of the same app again, it should be possible."
- **Apps give data points, the dashboard builds tiles** (widget contract v2, `docs/plan.md`). Apps never decide what a tile looks like.
- **Edit mode:** "when you click an edit button, everything is resizable and drag and drop, so you can change the positions." Done saves and locks.
- **Public repo, no private data, MIT licence.** Public from the first commit.
- **Rust server + plain JavaScript frontend, no build step.**
- **Third-party code gate:** before adding ANY crate, npm package or vendored JS file (e.g. a grid/drag library), name it, its publisher/maintainer, maturity, how it updates and what it can reach, then wait for Olivier's go. Mainstream, organisation-maintained crates only. `Cargo.lock` committed; vendored JS pinned to an exact version with its licence file.
- **No LLM, no metered API.**
- **Built with Fable 5.2** (Olivier's test of the model on a whole project).

## Run
- `cargo test`: unit tests, the demo app and the dashboard over real HTTP, and the checks that keep `SPEC.md`, `schema/` and the demo app in step. A few tests wait about a second on purpose (cache expiry).
- `cargo run -- --demo`: dashboard on `http://127.0.0.1:4700` + demo app (made-up data) on `http://127.0.0.1:4701/widgets`. No config, nothing saved.
- `cargo run -- --config config.toml`: the dashboard with a real config (format: `config.example.toml`). `--demo-app` also runs the demo app in the same process; `--listen` wins over the file.
- `cargo run -- --demo --web-dir web`: serve `web/` from disk, so a frontend change shows on reload (no rebuild).
- `node --test "tests/web/*.test.js"`: the frontend's own tests (pure logic). `cargo test` runs them too when Node is installed; no package is ever installed.
- `node tools/look.mjs [url] [folder]`: looks at a running dashboard in a headless Edge/Chrome: pictures (dark, light, phone), reads the sparkline with pointer and keyboard, reports anything sticking out of a tile and any page error. **Do this after every frontend change**, then open the pictures.
- `node tools/try-edit.mjs [url] [folder]`: uses edit mode in a headless browser (drag, resize, keys, add, change, remove, cancel, save, save conflict) and checks what got saved. Needs a **freshly started** `--demo`.
- Before a commit: `cargo fmt` and `cargo clippy --all-targets -- -D warnings`.

## Code map
- `SPEC.md` + `schema/*.schema.json`: widget contract v2 (source of truth).
- `src/contract.rs`: the contract as Rust types, `problems()` checks, `status_of()` (thresholds).
- `src/demo.rs`: the demo app (catalogue, made-up data as a pure function of the clock, HTTP routes, starter layout).
- `src/config.rs`: `config.toml` (listen, layout path, apps with `base_url`, `public_url`, `token`).
- `src/client.rs`: the HTTP client that calls apps (GET only, http:// only, 5 s, 1 MB, no redirects).
- `src/hub.rs`: catalogue + data cache per app; checks every entry and works out statuses for the browser.
- `src/layout.rs`: the layout document, its checks, and the JSON file store (`rev` guards against lost saves).
- `src/dashboard.rs`: the browser's API (`/api/apps`, `/api/data`, `/api/layout`) and the frontend routes.
- `src/web.rs` + `web/`: the frontend files, built into the binary. A new file needs an entry in `web.rs` (a test checks it).
  - `web/app.js`: loads layout + app list, polls per app, redraws changed tiles. `web/model.js`: what each tile shows (pure). `web/format.js`: numbers, units, ages (pure). `web/views.js`: the five views, status marks, the sparkline. `web/style.css`: the look.
  - `web/edit.js`: edit mode (drag, resize, keyboard, add, change, remove, Done / Cancel). `web/grid.js`: where tiles go when one moves (pure). `web/picker.js`: the dialog that chooses what a tile shows, built from the catalogues, with a live preview.
- `src/rfc3339.rs`: date-time format/parse without a date crate.
- `tests/spec.rs`: small JSON Schema checker; `tests/http.rs`: the demo app; `tests/dashboard.rs`: the dashboard against the demo app and a switchable fake app; `tests/web/`: frontend logic (Node's built-in runner), started by `tests/frontend.rs`.
- `tools/browser.mjs` + `tools/look.mjs` + `tools/try-edit.mjs`: drive a headless browser over its DevTools protocol (Node only, no packages). Development only.

## Look (keep unless Olivier changes it)
- **A wall of tiles:** flat faces set tight, the board showing between them like grout. No shadows, no floating cards.
- **One typeface** (Bahnschrift where the system has it, the system sans elsewhere). Numbers in ink, never in a status colour.
- **Colour does three jobs only:** ink for text, one blue for chart lines, green/amber/red for status. Status is always shape + word + colour.
- **Nothing jumps:** values refresh in place; old values stay, dimmed, with a note saying since when.

## Deploy
Runs on the owner's home server, reachable over Tailscale only. Details in `.private/NOTES.md`.

## Gotchas
- `SPEC.md` must hold exactly two fenced `json` blocks (the catalogue example, then the data example): `tests/spec.rs` reads them. The catalogue example must stay a cut-down copy of the demo app's catalogue.
- The schema checker in `tests/spec.rs` panics on a JSON Schema keyword or `pattern` it does not know. Using a new one in `schema/` means teaching the checker first.
- Crate versions were seeded from an already-vetted lock file. `cargo update` changes what runs: only do it on purpose, and say so.
- `Cargo.lock` lists 4 crates that are never compiled (`indexmap`, `hashbrown`, `equivalent`, `toml_writer`: optional parts of `toml`). `cargo tree -i <crate>` shows what is really built.
- The page is served with a strict Content-Security-Policy: no inline `<script>`, no `style="..."`, nothing loaded from another site. A frontend change that needs one of those will fail silently in the browser. Setting styles from JavaScript (`element.style.left = ...`) is allowed.
- No HTTPS client: `base_url` must be `http://`. Calling `https://` apps needs a TLS crate, which is a third-party gate question.
- In tests, the fake app's source refreshes every second: to get a fresh call without sleeping, ask with a new `who` value.
- After a failed call the hub answers at once with what it has kept and retries in the background. So recovery shows on the request *after* the retry: a test must ask twice.
- On Windows a refused connection takes about 2 s (Linux refuses at once). "App is down" looks slow here on first contact only.
- `node --test` wants a quoted glob (`"tests/web/*.test.js"`), not a folder.
- A full-page picture is scaled down when read back: thin lines and small marks can look grey or merged. Take an enlarged `clip` (see `shot` in `tools/browser.mjs`) before judging them.
- CSS: `.plot > svg`, not `.plot svg`. The status marks are `svg` too, and a selector that catches every `svg` stretches them over their label.
- A one-row tile (`h` = 1) has its own layout rules at the end of `style.css` (`@container (max-height: 120px)`). Check new view content there too.
- `drawTile` redraws only the tile's `.face`. Edit mode's controls (`.tools`, `.grip`) are its siblings: anything put inside `.face` is wiped on the next refresh.
- A tile's title is an `h2`, also inside the picker's preview. Dialog styles must target direct children (`.picker > h2`).
- On a packed wall, moving a tile down by one cell changes nothing (it rises back). Dragging swaps once the tile has gone down by its own height; the arrow keys use `nudge`, which keeps going until the tile lands somewhere new.
