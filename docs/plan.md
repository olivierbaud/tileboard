# tileboard plan

## What and why
One page where you choose, resize and move tiles fed by all your apps: server health next to trail status next to this week's spending. Every app exposes the same two read-only JSON endpoints; the dashboard draws everything, so the look is consistent and no app's code runs inside the dashboard.

Owner's words:
- "I would like to be able to choose what data you get from each app. Or even like, you know, putting 2 different widgets from the same app with different data."
- "If you want just a widget with just 1 data, and another widget with another data, from the same app, then another with 2 datas of the same app again, it should be possible."
- "When you click an edit button, everything is resizable and drag and drop, so you can change the positions."

**What makes it different** from Homarr / Homepage / Glance / Grafana (checked 2026-11-24): those all need a hand-written template, YAML mapping or query per widget. Here apps **describe themselves**, so the dashboard builds the picker: tick the fields you want, no template or config per tile.

## v1 done
- Public repo, MIT, `SPEC.md` for contract v2 (+ JSON Schema files).
- Demo widget server with made-up data, so anyone can run the whole thing locally (`--demo`).
- Dashboard: view mode with auto-refresh; edit mode with drag, resize, add, delete, pick fields and view; layout saved server-side; phone shows tiles stacked.
- Running on the owner's home server (Tailscale only) against the demo app + at least one real app.
- README with screenshots of demo data only.

## Widget contract v2 (what every app implements)
- **Catalogue:** `GET /widgets` →
  `{contract: 2, app, title, sources: [{id, title, refresh_s, params: [{name, label?, type, options, default}], fields: [{id, label, type, unit?, warn?, crit?}]}], presets?}`
  - **field** = one data point. `type`: `number` · `text` · `status` (ok/warn/crit) · `series` (sparkline) · `list` (items `{label, value?, status?}`).
  - **source** = a group of fields returned by one fetch.
  - **param** `type`: `choice` (one of `options`) · `text` · `number`. Every param has a `default`, so every source works with no query string.
  - **thresholds:** `crit` above `warn` = higher is worse; `crit` below `warn` = lower is worse. A `status` sent with the value wins.
  - **presets** (optional) = suggested tiles for one-click adding: `{title, source, fields, params?, view?}`.
- **Data:** `GET /widgets/<source>?<params>` → `{updated_at, link?, values: {<field>: {value, status?, series?}}}`. `link` is a full http(s) address or a path under the app's address. `null` = no data right now.
- Errors: `400` bad param value, `401` bad token, `404` unknown source.
- Read-only. Auth: none required on a private network; optional `Authorization: Bearer <token>` per app, configured in the dashboard's private config.
- `SPEC.md` is the source of truth; this section is the summary.

## Dashboard model
- **Tile** = `{id, fields: [{app, source, params, field}], view, label, x, y, w, h}`. View: big number · list · sparkline · table · status light. One field or many; same app or field in any number of tiles. The model allows mixing apps in one tile; the v1 picker keeps one app per tile.
- **One call per source, not per tile:** the browser names the (app, source, params) it needs in one `POST /api/data`; the server keeps each answer for `refresh_s`, so any number of tiles and browsers cost one call. The browser only ever talks to the dashboard server (tokens stay server-side, no CORS).
- **The server does the contract work:** it checks every entry against the catalogue, works out each value's status (thresholds) and turns `link` into a full address. The frontend only draws.
- **Config (git-ignored `config.toml`):** `listen`, `layout` path, and the list of apps (`name`, `base_url`, optional `public_url` for links, optional `token`). `config.example.toml` with demo values is committed. A tile's `app` is the config `name`.
- **Layout:** one JSON file (`layout.json`, git-ignored), saved whole. Why not SQLite: it is one small document with nothing to query, a file needs no extra crate, and the backup is copying it. A `rev` number refuses a save that started from an older layout (two browsers open).
- **Grid:** 12 columns; `x`, `w` in columns, `y`, `h` in rows.
- **Browser API:** `GET /api/apps` (catalogues + reachable or not), `POST /api/data`, `GET` / `PUT /api/layout`.

## Steps (deliverables in order)
1. **`SPEC.md` + demo widget server.** DONE 2026-11-24. Contract v2 written up with examples and JSON Schema; a demo app (made-up "server health", "weather", "spending" sources with moving fake values, sparkline series, a status that flips). Push public.
2. **Dashboard server.** DONE 2026-11-24. Reads config, fetches each app's catalogue, grouped + cached data endpoint for the browser, layout load/save, serves the static frontend. Tests against the demo app.
3. **View mode.** DONE 2026-11-24. Tiles render in the five views, auto-refresh, stale / error state per tile.
4. **Edit mode.** DONE 2026-11-24. Edit button → drag + resize every tile, add-tile picker (app → source → fields → view), delete, Done saves. Drag/resize library goes through the third-party gate first (candidates: gridstack.js, or our own code).
5. **Phone layout + README + privacy check.** DONE 2026-11-24. Stacked view under a width breakpoint; README with demo screenshots; review every committed file and the history for anything personal.
6. **Deploy + first real app.** DEPLOYED 2026-11-24 with the demo app; the first real app and the backup are still open (see the handover). Home server, Tailscale only, layout in the backup. A real app adds `/widgets` in its own repo session (not from here).

7. **Connect an app from the settings.** DONE 2026-11-24 (asked for by the owner after Step 6). Settings → Connect an app: one address (`/connect.md`) with everything a developer or an agent needs; a checker that calls an app and says what is wrong; adding and removing apps from the page or by one call, kept in `apps.json`.

## Decisions log
- 2026-11-23: widget contract adopted for all new apps (v1 shape).
- 2026-11-24: contract v2 (apps give fields, dashboard builds tiles), unlimited tiles, edit mode, public repo with no private data, MIT ("2, MIT"), Rust ("1 rust"), name `tileboard` ("2 tileboard"), built with Fable 5.2 ("we will go Fable 5.2 full project just to test").
- 2026-11-24 (Step 1): crates approved by the owner: `tokio`, `axum` (tokio-rs), `serde`, `serde_json` (serde-rs); axum with only `http1`, `json`, `query`, `tokio`. No date, random, regex or CLI crate: those few lines are our own code.
- 2026-11-24 (Step 1): spec details decided while writing it: a required `"contract": 2` marker; schemas are strict (an unknown property is an error) while dashboards ignore unknown properties; number formatting and unit placement are the dashboard's job (apps send rounded numbers); presets take fields from one source.
- 2026-11-24 (Step 1): the demo app is stateless (every value is a function of the clock) and listens on `127.0.0.1:4701` by default.
- 2026-11-24 (Step 2): crates approved by the owner: `hyper-util` + `http-body-util` (hyperium) as the HTTP client, **plain `http://` only** (chosen over reqwest + rustls: 6 extra crates instead of 94); `toml` (toml-rs) for the config file. Calling `https://` apps later means a TLS crate and a new gate question.
- 2026-11-24 (Step 2): layout in a JSON file, not SQLite (reasons in "Dashboard model"). 12-column grid. `--demo` = dashboard + demo app + starter layout, nothing saved; `--demo-app` runs the demo app next to a real config.
- 2026-11-24 (Step 2): the frontend files are built into the binary (one file to deploy); the page is served with a strict Content-Security-Policy.
- 2026-11-24 (Step 2): limits: an app gets 5 s and 1 MB per answer, redirects are not followed, a failed call is retried after 15 s at most, a good catalogue is re-read every 5 minutes (or on `?refresh=1`).
- 2026-11-24 (Step 3): the look: a wall of flat tiles with grout gaps, one typeface, ink numbers, status as shape + word + colour (details in `CLAUDE.md`, "Look"). Light and dark follow the system.
- 2026-11-24 (Step 3): what counts as old: a tile is marked "Not updating" when the last call to its app failed (last good values stay, dimmed), and "Old data" when the app has missed three refreshes plus 30 s.
- 2026-11-24 (Step 3): the page asks per app (a slow app does not hold the others), as often as the app's fastest source on screen (2 s to 60 s), and stops while the tab is hidden. It draws the board from the layout at once and fills in as answers arrive.
- 2026-11-24 (Step 3): server change found by looking at a dead app: after a failed call the hub answers at once from what it has kept and retries in the background. Before, every page load waited for the dead app.
- 2026-11-24 (Step 3): number formatting: money before the number and with cents unless whole; `%` and degrees touch the number, other units get a thin space; a list column gets one number of decimals; big standalone values are shortened (12.9K) with the full value on hover.
- 2026-11-24 (Step 3): a first phone layout came with it (two columns, small tiles side by side). Step 5 still owns the polish.
- 2026-11-24 (Step 3): no third-party code added. Frontend tests run on Node's built-in runner; the browser checks drive the Edge/Chrome already on the machine (`tools/`).
- 2026-11-24 (Steps 4 to 6): the owner was away and said to go on without his decisions. Choices below marked **(mine)** were made without him and are his to overturn.
- 2026-11-24 (Step 4, mine): drag and resize are our own code, not gridstack.js. It needs no third-party gate (which still stands), the grid is 12 fixed columns, and the page's security policy would block a library that injects styles.
- 2026-11-24 (Step 4, mine): tiles are always packed upward, like a wall tiled from the top: a moved or resized tile takes its place and the others make room; no tile can overlap another or float over a hole. A new tile goes into the first free place.
- 2026-11-24 (Step 4, mine): the picker lets one tile take fields from several sources, and from several apps (the plan said one app per tile for v1; the model always allowed more, and it cost nothing). Fields are ticked per source, each source with its own params; the app's presets are one-click.
- 2026-11-24 (Step 4, mine): Done saves and locks; Cancel puts everything back. If another screen saved meanwhile, Done asks which layout to keep. Removing a tile asks nothing: Cancel is the undo.
- 2026-11-24 (Step 4, mine): keyboard: arrow keys move the focused tile, Shift with arrows resizes, Enter changes it, Delete removes it. Under 720 px wide, tiles can be added, changed and removed but not dragged or resized.
- 2026-11-24 (Step 5, mine): phone layout: two columns in reading order; a small tile showing one value or one light takes half the width, lists, tables and charts take all of it.
- 2026-11-24 (Step 5, mine): README written for strangers: what it is, try it, show your own app, keep it private, how it is built, known limits. It names no other dashboard project (the comparison in "What and why" above is the owner's research, not re-checked). Its two JSON examples are tested against the spec. Pictures in `docs/images/` come from `tools/pictures.mjs` and the demo only.
- 2026-11-24 (Step 5): privacy review of every file and the whole history: nothing personal found. The only name is the owner's in `LICENSE` and `CLAUDE.md` (his own scaffold). Every commit uses the GitHub noreply address.
- 2026-11-24 (Step 6, mine): deployed on the home server the way the owner's other small apps are: a static Linux binary built on the work PC, a user-level service (no root), listening on the VPN address only, with the demo app in the same process. Checked: not reachable on the home network address or on loopback. The deploy script and real addresses are in `.private/`.
- 2026-11-24 (Step 6): left for the owner: adding the layout file to the nightly backup (that script belongs to another project and is production; a ready patch is in the private notes), and the first real app (each app gains `/widgets` in its own repo).
- 2026-11-24 (Step 6): the backup is done, with the owner's go ("1"): the layout, the apps added from the settings page and the config are copied by the home server's nightly backup. The changed script was first run there without uploading, and the archive checked.
- 2026-11-24 (Step 7): owner: "we need to find a way to connect an app. something in the settings. that could give any agent or developer what they need to connect." He also picked which of his apps is connected first (named in the private notes).
- 2026-11-24 (Step 7, mine): the kit is one plain-text address so an agent can simply be told to read it; it carries the dashboard's own address, the steps, the easy mistakes and the whole contract. The checker and adding apps are off unless `manage_apps = true`, because the dashboard has no login and both make the server call an address someone names. An app must pass the check to be added. Apps added this way live in `apps.json` (not `config.toml`, whose comments a program would destroy); an app of the same name in `config.toml` wins.
- 2026-11-24 (Step 5, mine): the demo's made-up day now opens with some spending, so the "Spent per day" tile never shows $0 in a first look. `rust-version = "1.88"` is declared (the code needs it).
