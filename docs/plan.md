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
  `{app, title, sources: [{id, title, params: [{name, type, options, default}], refresh_s, fields: [{id, label, type, unit, warn, crit}]}], presets?}`
  - **field** = one data point. `type`: `number` · `text` · `status` (ok/warn/crit) · `series` (sparkline) · `list`.
  - **source** = a group of fields returned by one fetch.
  - **presets** (optional) = suggested tiles for one-click adding.
- **Data:** `GET /widgets/<source>?<params>` → `{updated_at, link, values: {<field>: {value, status?, series?}}}`
- Read-only. Auth: none required on a private network; optional `Authorization: Bearer <token>` per app, configured in the dashboard's private config.
- `SPEC.md` is the source of truth once written; keep this section as the summary.

## Dashboard model
- **Tile** = `{id, fields: [{app, source, params, field}], view, label, x, y, w, h}`. View: big number · list · sparkline · table · status light. One field or many; same app or field in any number of tiles. The model allows mixing apps in one tile; the v1 picker keeps one app per tile.
- **One call per source, not per tile:** the server groups tiles by (app, source, params) and caches per `refresh_s`. The browser only ever talks to the dashboard server (tokens stay server-side, no CORS).
- **Config (git-ignored):** the list of apps (`name`, `base_url`, optional token). Example file with demo values is committed.
- **Layout:** stored server-side (JSON file or SQLite; the repo session decides and says why), git-ignored.

## Steps (deliverables in order)
1. **`SPEC.md` + demo widget server.** Contract v2 written up with examples and JSON Schema; a demo app (made-up "server health", "weather", "spending" sources with moving fake values, sparkline series, a status that flips). Push public.
2. **Dashboard server.** Reads config, fetches each app's catalogue, grouped + cached data endpoint for the browser, layout load/save, serves the static frontend. Tests against the demo app.
3. **View mode.** Tiles render in the five views, auto-refresh, stale / error state per tile.
4. **Edit mode.** Edit button → drag + resize every tile, add-tile picker (app → source → fields → view), delete, Done saves. Drag/resize library goes through the third-party gate first (candidates: gridstack.js, or our own code).
5. **Phone layout + README + privacy check.** Stacked view under a width breakpoint; README with demo screenshots; review every committed file and the history for anything personal.
6. **Deploy + first real app.** Home server, Tailscale only, layout in the backup. A real app adds `/widgets` in its own repo session (not from here).

## Decisions log
- 2026-11-23: widget contract adopted for all new apps (v1 shape).
- 2026-11-24: contract v2 (apps give fields, dashboard builds tiles), unlimited tiles, edit mode, public repo with no private data, MIT ("2, MIT"), Rust ("1 rust"), name `tileboard` ("2 tileboard"), built with Fable 5.2 ("we will go Fable 5.2 full project just to test").
