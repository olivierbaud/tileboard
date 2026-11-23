// What each tile shows, worked out from the layout, the catalogues and the data.
// Pure functions: no page, no network.

import { formatAge } from "./format.js";

/** One name for "this source with these params", whatever order the params are written in. */
export function sourceKey({ app, source, params = {} }) {
  const sorted = Object.keys(params)
    .sort()
    .map((name) => [name, params[name]]);
  return JSON.stringify([app, source, sorted]);
}

/** The distinct sources a layout needs, grouped by app: Map<app, [{ key, app, source, params }]>. */
export function sourcesByApp(layout) {
  const byApp = new Map();
  const seen = new Set();
  for (const tile of layout.tiles) {
    for (const { app, source, params = {} } of tile.fields) {
      const key = sourceKey({ app, source, params });
      if (seen.has(key)) continue;
      seen.add(key);
      if (!byApp.has(app)) byApp.set(app, []);
      byApp.get(app).push({ key, app, source, params });
    }
  }
  return byApp;
}

/**
 * How often to ask for an app's data, in seconds: as often as its fastest source
 * on screen, but never faster than every 2 s or slower than once a minute.
 * (Asking early is free: the server never calls an app more often than its refresh.)
 */
export function pollSeconds(sources, app) {
  const refreshes = sources.map(
    (wanted) => app?.catalogue?.sources.find((source) => source.id === wanted.source)?.refresh_s ?? 30,
  );
  const fastest = refreshes.length > 0 ? Math.min(...refreshes) : 30;
  return Math.min(60, Math.max(2, fastest));
}

const STATUS_RANK = { ok: 1, warn: 2, crit: 3 };

/** The worst of some statuses ("crit" beats "warn" beats "ok"), or null if there is none. */
export function worstStatus(statuses) {
  let worst = null;
  for (const status of statuses) {
    if (STATUS_RANK[status] > (STATUS_RANK[worst] ?? 0)) worst = status;
  }
  return worst;
}

/**
 * Is a source's data still good? `nowMs` is the server's clock.
 * - "failed": the last call did not work; the values are the last good ones.
 * - "old": the call works, but the app itself has missed three refreshes (plus 30 s of slack).
 */
export function freshness(answer, nowMs) {
  if (!answer.ok) {
    return { kind: "failed", ageS: (nowMs - Date.parse(answer.fetched_at)) / 1000, reason: answer.error };
  }
  const ageS = (nowMs - Date.parse(answer.updated_at)) / 1000;
  if (ageS > 3 * answer.refresh_s + 30) return { kind: "old", ageS };
  return { kind: "fresh", ageS };
}

/** The view to draw: the tile's own, or one that fits if its fields cannot be drawn that way. */
export function pickView(view, types) {
  if (types.length === 0) return view;
  const has = (type) => types.includes(type);
  if (view === "sparkline" && !has("series")) return has("list") ? "list" : "number";
  if (view === "list" && !has("list")) return "table";
  if (view === "number" && types[0] === "list") return "list";
  return view;
}

/** Finds a tile's field in the catalogues: { spec } or { problem }. `apps` is Map<name, app from /api/apps>. */
function lookUp(apps, ref) {
  const app = apps.get(ref.app);
  if (!app) return { problem: `There is no app "${ref.app}" in this dashboard's config.` };
  if (!app.catalogue) {
    return { problem: app.error ? `${ref.app}: ${app.error}.` : `${ref.app} has not answered yet.` };
  }
  const source = app.catalogue.sources.find((candidate) => candidate.id === ref.source);
  if (!source) return { problem: `${app.catalogue.title} no longer has "${ref.source}".` };
  const spec = source.fields.find((candidate) => candidate.id === ref.field);
  if (!spec) return { problem: `${source.title} no longer has "${ref.field}".` };
  return { spec };
}

/**
 * Everything the page needs to draw one tile.
 *
 * - `fields`: the fields that can be drawn, each { ref, spec, entry }.
 * - `problems`: why the other fields cannot (texts for a person).
 * - `state`: "fresh", "stale" (old values, dimmed), "waiting" (no answer yet) or "error" (nothing to draw).
 * - `note`: for a stale tile, { text, detail }.
 * - `status`: the worst status on the tile, list items included.
 *
 * `apps` is null until the app list has arrived: the tile then shows its name and waits.
 */
export function tileModel(tile, apps, answers, nowMs) {
  if (!apps) {
    return {
      id: tile.id,
      title: tile.label,
      view: tile.view,
      fields: [],
      problems: [],
      state: "waiting",
      note: null,
      link: null,
      status: null,
    };
  }

  const fields = [];
  const problems = new Set();
  const stale = [];
  let waiting = false;
  let link = null;

  for (const ref of tile.fields) {
    const found = lookUp(apps, ref);
    if (found.problem) {
      problems.add(found.problem);
      continue;
    }
    const answer = answers.get(sourceKey(ref));
    if (!answer) {
      waiting = true;
      fields.push({ ref, spec: found.spec, entry: { value: null } });
      continue;
    }
    if (!answer.ok && Object.keys(answer.values).length === 0) {
      problems.add(`${ref.app}: ${answer.error}.`);
      continue;
    }
    const fresh = freshness(answer, nowMs);
    if (fresh.kind !== "fresh") stale.push(fresh);
    link ??= answer.link;
    fields.push({ ref, spec: found.spec, entry: answer.values[ref.field] ?? { value: null } });
  }

  let state = "fresh";
  let note = null;
  if (fields.length === 0) {
    state = "error";
  } else if (waiting) {
    state = "waiting";
  } else if (stale.length > 0) {
    state = "stale";
    const worst = stale.find((entry) => entry.kind === "failed") ?? stale[0];
    note =
      worst.kind === "failed"
        ? { text: `Not updating, ${formatAge(worst.ageS)}`, detail: `${worst.reason}. These are the last good values.` }
        : { text: `Old data, ${formatAge(worst.ageS)}`, detail: "The app has not refreshed this data for a while." };
  }

  const statuses = fields.flatMap(({ spec, entry }) => {
    const items = spec.type === "list" && Array.isArray(entry.value) ? entry.value.map((item) => item.status) : [];
    return [entry.status, ...items];
  });

  return {
    id: tile.id,
    title: tile.label || fields[0]?.spec.label || tile.fields[0]?.field || "",
    view: pickView(
      tile.view,
      fields.map(({ spec }) => spec.type),
    ),
    fields,
    problems: [...problems],
    state,
    note,
    link,
    status: worstStatus(statuses),
  };
}
