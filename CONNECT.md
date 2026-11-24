# Connect an app to tileboard

This page is for whoever makes an app show up on a tileboard dashboard: a developer, or an AI agent working in the app's code. Everything needed is here. Nothing else has to be read.

This is the copy kept in tileboard's repository, for reading before any dashboard runs, so it cannot know where yours is: `http://ADDRESS-OF-THE-DASHBOARD` below stands for its address. A running dashboard serves this same page at `/connect.md`, with its own address filled in and only the steps that apply to it (on the dashboard: Settings, then Connect an app). Give that one if you can.

No dashboard yet? In a copy of this repository, `cargo run -- --demo` (needs Rust) starts one at http://127.0.0.1:4700, with made-up data. It checks and adds apps, and saves nothing: good for trying an app out.

## The idea

The app answers two read-only JSON addresses. The first says what data it has. The second gives the values. The dashboard does the rest: its owner ticks the fields they want and builds tiles from them. The app never decides what a tile looks like.

| Address in the app | Answers |
|---|---|
| `GET /widgets` | the catalogue: the sources, and the fields of each |
| `GET /widgets/<source>` | the current values of one source |

## Do this, in order

1. **Choose what to offer.** Group values that are fetched together into a *source* (for example `server`, `orders`, `weather`). Each value is a *field* with a type: `number`, `text`, `status` (ok / warn / crit), `series` (a short history, drawn as a small chart) or `list`. Offer what the owner would glance at: current numbers, a state, a short history, a top-5 list. Never offer secrets: everything in a source can end up on a screen.
2. **Add the two addresses to the app**, following the contract below exactly. Keep them read-only and fast: answer from memory or a cache, never run a slow query per request. Use the app's existing way of serving HTTP. Add no new dependency if the app can already answer JSON.
3. **Check your work.** The dashboard's checker calls the app the way the dashboard will, and says exactly what is wrong. It answers only if the dashboard allows it (`manage_apps = true` in its `config.toml`); the test dashboard of `cargo run -- --demo` always does, at http://127.0.0.1:4700:

   ```sh
   curl -s -X POST http://ADDRESS-OF-THE-DASHBOARD/api/check -H "Content-Type: application/json" -d '{"base_url": "http://ADDRESS-OF-THE-APP"}'
   ```

   `ADDRESS-OF-THE-APP` is where the dashboard **server** reaches the app. It must start with `http://` (the dashboard cannot call `https://` yet). If the app and the dashboard run on the same machine, the app must listen on an address the dashboard can call. Add `"token": "..."` if the app asks for one. Repeat until the answer says `"ok": true`.
4. **Connect it.** If the dashboard lets apps be added from outside (`manage_apps = true` in its `config.toml`), register the app there. The dashboard checks it first and refuses an app that does not pass:

   ```sh
   curl -s -X POST http://ADDRESS-OF-THE-DASHBOARD/api/apps -H "Content-Type: application/json" -d '{"name": "my-app", "base_url": "http://ADDRESS-OF-THE-APP"}'
   ```

   `name` is the owner's short name for the app (lowercase letters, digits, `_`, `-`). Tiles refer to it, so it must not change later. Optional: `"public_url"` (where a browser reaches the app, if that differs, for the links on tiles) and `"token"`. The owner can do the same by hand on the dashboard: Settings, then Connect an app.

   If it does not, give its owner this block for the dashboard's `config.toml` (then they restart the dashboard):

   ```toml
   [[apps]]
   name = "my-app"                             # the owner's short name; tiles refer to it
   base_url = "http://ADDRESS-OF-THE-APP"
   # public_url = "..."                        # where a browser reaches the app, if that differs
   # token = "..."                             # only if the app asks for one
   ```
5. **Tell the owner** which sources and fields now exist. They add tiles on the dashboard: Edit, then Add tile.

## Rules that are easy to get wrong

- **Ids are forever.** `app`, source `id`, field `id`, param `name`: lowercase letters, digits, `_`, `-`. Saved tiles refer to them. Renaming one breaks those tiles: add a new one and keep the old.
- **`"contract": 2`** must be in the catalogue.
- **`updated_at`** is when the app last refreshed the data, not when the request arrived. RFC 3339, UTC, like `2026-11-24T09:30:05Z`.
- **`refresh_s`** is how often the data is worth fetching again. The dashboard never asks more often.
- **Every field, every time.** Return all the fields of the source. `"value": null` means "no data right now".
- **Thresholds are optional and belong to the catalogue:** `warn` and `crit` on a `number` or `series` field. `crit` above `warn` means higher is worse; `crit` below `warn` means lower is worse (free disk space).
- **Send numbers rounded** to the precision you mean. The dashboard formats them and places the unit.
- **Every param has a default**, so every source answers with no query string. A bad value for a param is a `400`; an unknown source is a `404`.
- **Limits:** the dashboard gives up after 5 seconds, refuses an answer over 1 MB, and does not follow redirects.
- **Private network.** No login is needed by default. If the app must not be read by everything on its network, require `Authorization: Bearer <token>` on both addresses and give the token when connecting.

## The contract

Below is the full contract. The JSON Schemas are at http://ADDRESS-OF-THE-DASHBOARD/schema/catalogue.schema.json and http://ADDRESS-OF-THE-DASHBOARD/schema/data.schema.json.

---

# Widget contract v2

How an app offers its data to a tileboard dashboard.

An app implements **two read-only JSON endpoints**. The dashboard does everything else: the picker, the drawing, the layout, the refresh. Apps never decide what a tile looks like.

| Endpoint | Returns |
|---|---|
| `GET /widgets` | the **catalogue**: what data the app has |
| `GET /widgets/<source>` | the **data**: current values of one source |

- This file is the source of truth for the contract.
- JSON Schema: [`schema/catalogue.schema.json`](schema/catalogue.schema.json) and [`schema/data.schema.json`](schema/data.schema.json).
- Working example: the demo app in this repo (`cargo run -- --demo`, made-up data).

MUST, SHOULD and MAY are used as in RFC 2119.

## Words

| Word | Meaning |
|---|---|
| **app** | A program that has data to show. It implements this contract. |
| **field** | One data point: a number, a text, a status, a series or a list. |
| **source** | A group of fields returned by one fetch. |
| **param** | A choice that changes what a source returns (which host, which period). |
| **preset** | A tile the app suggests, for one-click adding. Only a suggestion. |
| **dashboard** | tileboard. It reads catalogues, fetches data and draws tiles. |

## Rules for both endpoints

- **Read-only.** `GET` only. A request MUST NOT change anything in the app.
- **JSON.** Answer `200` with `Content-Type: application/json`, UTF-8.
- **Fast.** Answer from memory or a cache, well under 2 seconds. Do not run a slow query per request. The dashboard gives up after 5 seconds.
- **Small.** The dashboard refuses an answer larger than 1 MB.
- **No redirects.** The dashboard does not follow them: answer at the address itself.
- **Base address.** The dashboard is configured with the app's base address (for example `http://app.example:8080` or `http://host.example/my-app`). Both endpoints sit under it.
- **Only this spec.** An app MUST NOT add properties that are not in this document. A dashboard MUST ignore properties it does not know, so later additions do not break older dashboards.
- **No CORS needed.** The dashboard server calls the app; the browser never does.

### Ids

`app`, source `id`, field `id` and param `name` are **ids**: lowercase letters, digits, `_` and `-`, starting with a letter or digit, at most 64 characters (`^[a-z0-9][a-z0-9_-]{0,63}$`).

**Ids are forever.** Saved layouts refer to them. Renaming an id breaks every tile that uses it. Add a new one instead and keep the old one working.

## Catalogue: `GET /widgets`

```json
{
  "contract": 2,
  "app": "demo",
  "title": "Demo app",
  "sources": [
    {
      "id": "server",
      "title": "Server health",
      "refresh_s": 5,
      "params": [
        {
          "name": "host",
          "label": "Host",
          "type": "choice",
          "options": [
            { "value": "atlas", "label": "Atlas" },
            { "value": "comet", "label": "Comet" }
          ],
          "default": "atlas"
        }
      ],
      "fields": [
        { "id": "cpu", "label": "CPU", "type": "number", "unit": "%", "warn": 75, "crit": 90 },
        { "id": "cpu_history", "label": "CPU history", "type": "series", "unit": "%", "warn": 75, "crit": 90 },
        { "id": "disk_free", "label": "Disk free", "type": "number", "unit": "GB", "warn": 50, "crit": 20 },
        { "id": "uptime", "label": "Uptime", "type": "text" },
        { "id": "backup", "label": "Last backup", "type": "status" },
        { "id": "services", "label": "Services", "type": "list" }
      ]
    }
  ],
  "presets": [
    { "title": "CPU now", "source": "server", "fields": ["cpu"], "view": "number" },
    { "title": "Server overview", "source": "server", "fields": ["cpu", "disk_free", "uptime"], "view": "table" }
  ]
}
```

### Catalogue

| Property | Type | Required | Meaning |
|---|---|---|---|
| `contract` | integer | yes | Always `2`. Anything else is not a v2 catalogue. |
| `app` | id | yes | The app's machine name. |
| `title` | string | yes | The app's name as people see it. |
| `sources` | array of source | yes, at least 1 | What can be fetched. |
| `presets` | array of preset | no | Suggested tiles. |

### Source

| Property | Type | Required | Meaning |
|---|---|---|---|
| `id` | id | yes | Last part of the data address: `/widgets/<id>`. Unique in the catalogue. |
| `title` | string | yes | Name shown in the picker. |
| `refresh_s` | integer, 1 or more | yes | How often the data is worth fetching again, in seconds. The dashboard never fetches more often than this. |
| `params` | array of param | no | Choices that change the result. |
| `fields` | array of field | yes, at least 1 | The data points this source returns. |

### Param

| Property | Type | Required | Meaning |
|---|---|---|---|
| `name` | id | yes | Name in the query string. Unique in the source. |
| `label` | string | no | Name shown in the picker. Defaults to `name`. |
| `type` | `choice` · `text` · `number` | yes | `choice`: one of `options`. `text`: free text. `number`: a number written as text (`"7"`, `"2.5"`). |
| `options` | array of `{value, label?}` | `choice` only, at least 1 | The allowed values. `label` defaults to `value`. |
| `default` | string | yes | The value used when the param is left out. MUST be valid (for `choice`: one of the option values). |

Param values are always strings, because they travel in the query string. Every param has a default, so every source can be fetched with no query string at all.

### Field

| Property | Type | Required | Meaning |
|---|---|---|---|
| `id` | id | yes | Key in the data's `values`. Unique in the source. |
| `label` | string | yes | Name shown in the picker and on tiles. |
| `type` | `number` · `text` · `status` · `series` · `list` | yes | See [Field types](#field-types). |
| `unit` | string | no | `number`, `series` and `list` only. Short: `%`, `°C`, `GB`, `$`. Where it is drawn is the dashboard's job. |
| `warn`, `crit` | number | no | `number` and `series` only. See [Status and thresholds](#status-and-thresholds). |

Send numbers rounded to the precision you mean. The dashboard does not invent precision.

### Preset

| Property | Type | Required | Meaning |
|---|---|---|---|
| `title` | string | yes | Suggested tile label. |
| `source` | id | yes | A source of this catalogue. |
| `fields` | array of id | yes, at least 1 | Fields of that source, no repeats. |
| `params` | object, string to string | no | Param values. Left-out params use their default. |
| `view` | `number` · `list` · `sparkline` · `table` · `status` | no | Suggested view. |

A preset is only a starting point: once added, the tile belongs to the person and they can change everything. If `view` does not fit the fields, the dashboard picks one that does.

| View | Shows | Fits |
|---|---|---|
| `number` | the value, big | `number`, `text`, `series` (current value) |
| `sparkline` | a small line chart | `series` |
| `list` | the items | `list` |
| `table` | one row per field | any |
| `status` | a coloured light | `status`, or any field that has a status |

### Things the schema cannot check

A catalogue MUST also follow these. The dashboard checks them.

- Source ids are unique. Field ids and param names are unique inside their source.
- A `choice` default is one of its option values; option values are unique. A `number` default is a number.
- If a field has both `warn` and `crit`, they differ.
- A preset's `source` exists, its `fields` exist in that source, and its `params` are params of that source with valid values.

## Data: `GET /widgets/<source>`

Query string: the source's params by `name`, for example `/widgets/server?host=comet`.

- A left-out param uses its `default`.
- An invalid value for a known param: answer `400`.
- A query param the source does not have: ignore it.

```json
{
  "updated_at": "2026-11-24T09:30:05Z",
  "link": "/servers/atlas",
  "values": {
    "cpu": { "value": 41.5 },
    "cpu_history": { "value": 41.5, "series": [38.2, 40.1, 44.9, 43.0, 41.5] },
    "disk_free": { "value": 18.4 },
    "uptime": { "value": "12d 4h" },
    "backup": { "value": "warn" },
    "services": {
      "value": [
        { "label": "web", "status": "ok" },
        { "label": "database", "status": "ok" },
        { "label": "mail", "value": "restarting", "status": "warn" }
      ]
    }
  }
}
```

| Property | Type | Required | Meaning |
|---|---|---|---|
| `updated_at` | string, RFC 3339 date-time | yes | When the app last refreshed this data. Not when the request arrived. UTC (`Z`) preferred. The dashboard uses it to show stale data. |
| `link` | string | no | Where a click on the tile goes. Either a full `http://` / `https://` address, or a path starting with one `/`, which the dashboard puts behind the app's address: with the app at `http://host.example/my-app`, `/servers/atlas` becomes `http://host.example/my-app/servers/atlas`. Anything else is dropped. |
| `values` | object, field id to entry | yes | One entry per field. |

- Return **every field** of the source. A missing field is treated like `"value": null`.
- A key that is not a field of the source is ignored.

### Field types

Every entry has a `value`. `null` always means "no data right now".

| Field type | `value` | Also |
|---|---|---|
| `number` | number or `null` | |
| `text` | string or `null` | |
| `status` | `"ok"`, `"warn"`, `"crit"` or `null` | |
| `series` | number or `null`: the current value | `series` (required): array of numbers, oldest first, evenly spaced. `null` marks a gap. |
| `list` | array of items, or `null` | |

A **list item** is `{ "label": string, "value"?: number or string, "status"?: "ok" | "warn" | "crit" }`. The field's `unit` applies to numeric item values.

Any entry MAY also carry `"status": "ok" | "warn" | "crit"`. On a `status` field leave it out: the value is the status.

Keep it small: a series SHOULD have at most 200 points, a list at most 50 items, a text at most 200 characters.

## Status and thresholds

A value's status is `ok`, `warn`, `crit` or none. The dashboard works it out in this order:

1. **A `status` field:** its value is the status.
2. **The entry has `status`:** that wins.
3. **The field has `warn` and/or `crit`** and the value is a number:
   - `crit` is above `warn`, or only one of them is given: **higher is worse**. At or above `crit` is `crit`; at or above `warn` is `warn`; otherwise `ok`.
   - `crit` is below `warn`: **lower is worse**. At or below `crit` is `crit`; at or below `warn` is `warn`; otherwise `ok`.
4. **Otherwise:** no status.

In the example, `cpu` (`warn` 75, `crit` 90) at 41.5 is `ok`, and `disk_free` (`warn` 50, `crit` 20) at 18.4 is `crit`.

For a `series` field the thresholds apply to the current value; the dashboard may also draw them as lines.

## Errors

| Code | When |
|---|---|
| `400` | A param value is invalid. |
| `401` | The token is missing or wrong (only if the app uses one). |
| `404` | The source does not exist. |
| `5xx` | The app could not answer. |

The body SHOULD be `{ "error": "<short message for a person>" }`.

On any error or timeout the dashboard keeps the last good values, marks the tile, and tries again later.

A field whose entry does not fit its type is shown as "no data" with a note; the other fields of the source are not affected.

## Auth and safety

- **Default: no auth.** The contract is meant for a private network (home LAN, VPN). It only exposes what the app chooses to put in its sources.
- **Optional token.** An app MAY require `Authorization: Bearer <token>` on both endpoints and answer `401` without it. The token is set per app in the dashboard's private config. It never goes in an address.
- **Not on the open internet** without a token and HTTPS.
- **Do not put secrets in fields.** Everything in a source can end up on a screen.
- **Dashboards treat everything as plain text.** Labels, values and messages are never run as HTML. A `link` is only followed if it is `http`, `https` or a path.

## Changing an app over time

| Change | Safe? |
|---|---|
| Add a source, a field, a preset | Yes, any time. |
| Add a param (it has a default) | Yes. |
| Change a label, title, unit, threshold, `refresh_s`, preset | Yes. |
| Rename or remove an id (app, source, field, param, option value) | No: tiles using it break. Add a new one, keep the old. |
| Change a field's `type` | No. Add a new field. |

A breaking change to the contract itself becomes `"contract": 3`.

## Checklist for app authors

1. `GET /widgets` returns the catalogue with `"contract": 2`.
2. `GET /widgets/<source>` works with no query string for every source.
3. Every field in the catalogue is in `values`, with the right kind of `value`.
4. `updated_at` is the time the data was refreshed.
5. Both endpoints are read-only and fast.
6. Both answers pass the JSON Schemas in `schema/`.
7. Unknown source gives `404`; a bad param value gives `400`.

## Try it

```sh
cargo run -- --demo
curl http://127.0.0.1:4701/widgets
curl "http://127.0.0.1:4701/widgets/server?host=comet"
```

The demo app has three sources of made-up data (`server`, `weather`, `spending`) that use every field type, params, thresholds in both directions and presets. `--demo` also starts a dashboard on `http://127.0.0.1:4700` that shows it.

To check your own app, add it to `config.toml` (see `config.example.toml`) and open the dashboard: an app whose catalogue or data does not follow this document is listed with the reason.
