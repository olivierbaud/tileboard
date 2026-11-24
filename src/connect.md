# Connect an app to tileboard

{{ABOUT}}

## The idea

The app answers two read-only JSON addresses. The first says what data it has. The second gives the values. The dashboard does the rest: its owner ticks the fields they want and builds tiles from them. The app never decides what a tile looks like.

| Address in the app | Answers |
|---|---|
| `GET /widgets` | the catalogue: the sources, and the fields of each |
| `GET /widgets/<source>` | the current values of one source |

## Do this, in order

1. **Choose what to offer.** Group values that are fetched together into a *source* (for example `server`, `orders`, `weather`). Each value is a *field* with a type: `number`, `text`, `status` (ok / warn / crit), `series` (a short history, drawn as a small chart) or `list`. Offer what the owner would glance at: current numbers, a state, a short history, a top-5 list. Never offer secrets: everything in a source can end up on a screen.
2. **Add the two addresses to the app**, following the contract below exactly. Keep them read-only and fast: answer from memory or a cache, never run a slow query per request. Use the app's existing way of serving HTTP. Add no new dependency if the app can already answer JSON.
3. **Check your work.** {{CHECK}}

   `ADDRESS-OF-THE-APP` is where the dashboard **server** reaches the app. It must start with `http://` (the dashboard cannot call `https://` yet). If the app and the dashboard run on the same machine, the app must listen on an address the dashboard can call. Add `"token": "..."` if the app asks for one. Repeat until the answer says `"ok": true`.
4. **Connect it.** {{CONNECT}}
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

Below is the full contract. The JSON Schemas are at {{DASHBOARD}}/schema/catalogue.schema.json and {{DASHBOARD}}/schema/data.schema.json.

---

