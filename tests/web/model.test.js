// Run with: node --test "tests/web/*.test.js"   (`cargo test` does it too, when Node is installed)

import assert from "node:assert/strict";
import test from "node:test";

import {
  VIEWS,
  bestView,
  freshness,
  pickView,
  pollSeconds,
  presetContent,
  sourceKey,
  sourcesByApp,
  startSize,
  tileModel,
  viewFits,
  worstStatus,
} from "../../web/model.js";

// ---------------------------------------------------------------- fixtures

const NOW = Date.parse("2026-11-24T09:30:05Z");
const ago = (seconds) => new Date(NOW - seconds * 1000).toISOString();

/** A made-up app with one source, as `/api/apps` gives it. */
function shop(overrides = {}) {
  return {
    name: "shop",
    ok: true,
    error: null,
    catalogue: {
      contract: 2,
      app: "shop",
      title: "Shop",
      sources: [
        {
          id: "orders",
          title: "Orders",
          refresh_s: 10,
          fields: [
            { id: "count", label: "Orders today", type: "number", warn: 100, crit: 200 },
            { id: "trend", label: "Orders per hour", type: "series" },
            { id: "state", label: "Checkout", type: "status" },
            { id: "note", label: "Note", type: "text" },
            { id: "latest", label: "Latest orders", type: "list", unit: "$" },
          ],
        },
      ],
    },
    ...overrides,
  };
}

const apps = (...list) => new Map(list.map((app) => [app.name, app]));

/** A good answer for the shop's `orders` source, as `/api/data` gives it. */
function answer(overrides = {}) {
  return {
    app: "shop",
    source: "orders",
    params: {},
    ok: true,
    error: null,
    refresh_s: 10,
    fetched_at: ago(2),
    updated_at: ago(4),
    link: "http://shop.example/orders",
    values: {
      count: { value: 12, status: "ok" },
      trend: { value: 3, series: [1, 2, 3] },
      state: { value: "ok", status: "ok" },
      note: { value: "All quiet" },
      latest: { value: [{ label: "Order 1", value: 19.9 }, { label: "Order 2", value: "pending", status: "warn" }] },
    },
    ...overrides,
  };
}

const ref = (field, extra = {}) => ({ app: "shop", source: "orders", field, ...extra });
const tile = (view, fields, label = "") => ({ id: "t1", label, view, x: 0, y: 0, w: 3, h: 2, fields });
const answers = (value = answer()) => new Map([[sourceKey(ref("count")), value]]);

// ------------------------------------------------------------------ sources

test("a source is the same whatever order its params are written in", () => {
  const a = sourceKey({ app: "shop", source: "orders", params: { period: "week", top: "5" } });
  const b = sourceKey({ app: "shop", source: "orders", params: { top: "5", period: "week" } });
  assert.equal(a, b);
  assert.notEqual(a, sourceKey({ app: "shop", source: "orders", params: { period: "month", top: "5" } }));
  assert.notEqual(a, sourceKey({ app: "shop", source: "orders" }));
  assert.equal(sourceKey({ app: "shop", source: "orders" }), sourceKey({ app: "shop", source: "orders", params: {} }));
});

test("the sources a layout needs are found once each, grouped by app", () => {
  const layout = {
    tiles: [
      tile("number", [ref("count"), ref("note")]),
      tile("list", [ref("latest")]),
      tile("number", [ref("count", { params: { period: "week" } })]),
      tile("number", [{ app: "other", source: "x", field: "y" }]),
    ],
  };
  const byApp = sourcesByApp(layout);
  assert.deepEqual([...byApp.keys()], ["shop", "other"]);
  assert.deepEqual(
    byApp.get("shop").map(({ source, params }) => ({ source, params })),
    [
      { source: "orders", params: {} },
      { source: "orders", params: { period: "week" } },
    ],
  );
  assert.equal(byApp.get("other").length, 1);
});

test("an app is asked as often as its fastest source, within limits", () => {
  const wanted = (...ids) => ids.map((source) => ({ source }));
  const app = shop();
  app.catalogue.sources.push({ id: "slow", refresh_s: 3600, fields: [] }, { id: "instant", refresh_s: 1, fields: [] });
  assert.equal(pollSeconds(wanted("orders"), app), 10);
  assert.equal(pollSeconds(wanted("orders", "slow"), app), 10);
  assert.equal(pollSeconds(wanted("slow"), app), 60, "never slower than once a minute");
  assert.equal(pollSeconds(wanted("instant"), app), 2, "never faster than every 2 s");
  assert.equal(pollSeconds(wanted("gone"), app), 30, "a source the catalogue does not have");
  assert.equal(pollSeconds(wanted("orders"), undefined), 30, "an app that has no catalogue");
});

// ------------------------------------------------------------------- status

test("the worst status wins", () => {
  assert.equal(worstStatus(["ok", "warn", "ok"]), "warn");
  assert.equal(worstStatus(["warn", "crit", "ok"]), "crit");
  assert.equal(worstStatus(["ok", undefined, null]), "ok");
  assert.equal(worstStatus([undefined, null]), null);
  assert.equal(worstStatus([]), null);
});

test("data is fresh, old, or left over from a failed call", () => {
  assert.equal(freshness(answer(), NOW).kind, "fresh");
  // Old: the app has missed three refreshes (3 x 10 s) plus 30 s of slack.
  assert.equal(freshness(answer({ updated_at: ago(60) }), NOW).kind, "fresh");
  const old = freshness(answer({ updated_at: ago(61) }), NOW);
  assert.deepEqual([old.kind, Math.round(old.ageS)], ["old", 61]);
  // Failed: the age is how old the last good values are.
  const failed = freshness(answer({ ok: false, error: "the app answered 500", fetched_at: ago(120) }), NOW);
  assert.deepEqual(failed, { kind: "failed", ageS: 120, reason: "the app answered 500" });
});

// -------------------------------------------------------------------- views

test("a view that does not fit the fields falls back to one that does", () => {
  assert.equal(pickView("sparkline", ["series"]), "sparkline");
  assert.equal(pickView("sparkline", ["number", "series"]), "sparkline");
  assert.equal(pickView("sparkline", ["number"]), "number");
  assert.equal(pickView("sparkline", ["list"]), "list");
  assert.equal(pickView("list", ["list"]), "list");
  assert.equal(pickView("list", ["number", "text"]), "table");
  assert.equal(pickView("number", ["list"]), "list");
  assert.equal(pickView("number", ["number", "list"]), "number");
  for (const view of ["table", "status"]) {
    assert.equal(pickView(view, ["number", "text", "status", "series", "list"]), view);
  }
  assert.equal(pickView("sparkline", []), "sparkline", "nothing to draw: nothing to decide");
});

test("the picker offers a view only when it fits the chosen fields", () => {
  const offered = (types) => VIEWS.map(([id]) => id).filter((id) => viewFits(id, types));
  assert.deepEqual(offered(["number"]), ["number", "table", "status"]);
  assert.deepEqual(offered(["series"]), ["number", "sparkline", "table", "status"]);
  assert.deepEqual(offered(["list"]), ["list", "table", "status"]);
  assert.deepEqual(offered(["number", "list"]), ["number", "list", "table", "status"]);
  assert.deepEqual(offered([]), [], "nothing chosen: nothing to offer");
});

test("a new tile starts with the view that suits its fields", () => {
  assert.equal(bestView(["number"]), "number");
  assert.equal(bestView(["text"]), "number");
  assert.equal(bestView(["series"]), "sparkline");
  assert.equal(bestView(["list"]), "list");
  assert.equal(bestView(["status"]), "status");
  assert.equal(bestView(["number", "text"]), "number", "a main value with one more line");
  assert.equal(bestView(["number", "number", "text"]), "table");
  assert.equal(bestView(["number", "list"]), "list");
  // Whatever it picks can be drawn as it is.
  for (const types of [["number"], ["series"], ["list"], ["status"], ["text", "series"], ["list", "number"], ["a", "b", "c"]]) {
    assert.ok(viewFits(bestView(types), types), types.join(","));
  }
});

test("a new tile starts at a size that suits its view", () => {
  assert.deepEqual(startSize("number", 1), { w: 3, h: 2 });
  assert.deepEqual(startSize("number", 2), { w: 3, h: 3 });
  assert.deepEqual(startSize("sparkline", 1), { w: 5, h: 2 });
  assert.deepEqual(startSize("list", 1), { w: 3, h: 3 });
  assert.deepEqual(startSize("table", 3), { w: 4, h: 2 });
  assert.deepEqual(startSize("table", 6), { w: 4, h: 4 });
  assert.deepEqual(startSize("table", 40), { w: 4, h: 4 }, "never taller than 4 to start with");
  assert.deepEqual(startSize("status", 1), { w: 2, h: 2 });
  assert.deepEqual(startSize("status", 3), { w: 4, h: 2 });
});

test("an app's suggestion becomes what a tile shows", () => {
  const preset = { title: "Orders this week", source: "orders", params: { period: "week" }, fields: ["count", "trend"], view: "table" };
  assert.deepEqual(presetContent("shop", preset), {
    label: "Orders this week",
    view: "table",
    fields: [
      { app: "shop", source: "orders", params: { period: "week" }, field: "count" },
      { app: "shop", source: "orders", params: { period: "week" }, field: "trend" },
    ],
  });
  // No params, no view: the fields carry no params and the view is left to be chosen.
  assert.deepEqual(presetContent("shop", { title: "Orders", source: "orders", fields: ["count"] }), {
    label: "Orders",
    view: null,
    fields: [{ app: "shop", source: "orders", field: "count" }],
  });
});

// -------------------------------------------------------------------- tiles

test("a tile with fresh data", () => {
  const model = tileModel(tile("number", [ref("count"), ref("note")]), apps(shop()), answers(), NOW);
  assert.equal(model.state, "fresh");
  assert.equal(model.note, null);
  assert.deepEqual(model.problems, []);
  assert.equal(model.view, "number");
  assert.equal(model.title, "Orders today", "no label on the tile: the first field's label");
  assert.equal(model.link, "http://shop.example/orders");
  assert.equal(model.status, "ok");
  assert.deepEqual(
    model.fields.map(({ spec, entry }) => [spec.id, entry.value]),
    [
      ["count", 12],
      ["note", "All quiet"],
    ],
  );
});

test("the tile's own label is its title", () => {
  const model = tileModel(tile("number", [ref("count")], "Today"), apps(shop()), answers(), NOW);
  assert.equal(model.title, "Today");
});

test("the tile's status is its worst one, list items included", () => {
  const withList = tileModel(tile("list", [ref("latest")]), apps(shop()), answers(), NOW);
  assert.equal(withList.status, "warn");
  const critical = answer();
  critical.values.count.status = "crit";
  const model = tileModel(tile("table", [ref("count"), ref("latest")]), apps(shop()), answers(critical), NOW);
  assert.equal(model.status, "crit");
  const none = tileModel(tile("number", [ref("note")]), apps(shop()), answers(), NOW);
  assert.equal(none.status, null);
});

test("before the app list arrives a tile shows its name and waits", () => {
  const model = tileModel(tile("number", [ref("count")], "Today"), null, new Map(), NOW);
  assert.deepEqual(
    [model.title, model.state, model.fields, model.problems],
    ["Today", "waiting", [], []],
    "an app that is not known yet is not called missing",
  );
});

test("before the first answer a tile is waiting", () => {
  const model = tileModel(tile("number", [ref("count")]), apps(shop()), new Map(), NOW);
  assert.equal(model.state, "waiting");
  assert.deepEqual(model.fields.map(({ entry }) => entry), [{ value: null }]);
  assert.deepEqual(model.problems, []);
});

test("after a failed call the last good values stay, marked as not updating", () => {
  const failed = answer({ ok: false, error: "the app answered 500", fetched_at: ago(120) });
  const model = tileModel(tile("number", [ref("count")]), apps(shop()), answers(failed), NOW);
  assert.equal(model.state, "stale");
  assert.equal(model.fields[0].entry.value, 12);
  assert.deepEqual(model.note, {
    text: "Not updating, 2 min",
    detail: "the app answered 500. These are the last good values.",
  });
});

test("data the app has not refreshed for a while is marked as old", () => {
  const model = tileModel(tile("number", [ref("count")]), apps(shop()), answers(answer({ updated_at: ago(600) })), NOW);
  assert.equal(model.state, "stale");
  assert.equal(model.note.text, "Old data, 10 min");
});

test("a call that never worked leaves nothing to draw, and says why", () => {
  const never = answer({ ok: false, error: "cannot reach the app (connection refused)", values: {}, fetched_at: null });
  const model = tileModel(tile("number", [ref("count"), ref("note")]), apps(shop()), answers(never), NOW);
  assert.equal(model.state, "error");
  assert.deepEqual(model.fields, []);
  assert.deepEqual(model.problems, ["shop: cannot reach the app (connection refused)."], "said once, not per field");
});

test("an app, a source or a field that is not there is a problem in words", () => {
  const cases = [
    [{ app: "nope", source: "orders", field: "count" }, apps(shop()), 'There is no app "nope" in this dashboard\'s config.'],
    [ref("count"), apps(shop({ ok: false, error: "the app answered 401", catalogue: null })), "shop: the app answered 401."],
    [ref("count"), apps(shop({ catalogue: null })), "shop has not answered yet."],
    [{ app: "shop", source: "refunds", field: "count" }, apps(shop()), 'Shop no longer has "refunds".'],
    [ref("total"), apps(shop()), 'Orders no longer has "total".'],
  ];
  for (const [field, catalogue, expected] of cases) {
    const model = tileModel(tile("number", [field]), catalogue, answers(), NOW);
    assert.equal(model.state, "error");
    assert.deepEqual(model.problems, [expected]);
  }
});

test("one missing field does not hide the others", () => {
  const model = tileModel(tile("table", [ref("count"), ref("total")]), apps(shop()), answers(), NOW);
  assert.equal(model.state, "fresh");
  assert.deepEqual(model.fields.map(({ spec }) => spec.id), ["count"]);
  assert.deepEqual(model.problems, ['Orders no longer has "total".']);
});

test("a field the answer does not hold has no data", () => {
  const partial = answer();
  delete partial.values.note;
  const model = tileModel(tile("number", [ref("note")]), apps(shop()), answers(partial), NOW);
  assert.deepEqual(model.fields[0].entry, { value: null });
  assert.equal(model.state, "fresh");
});

test("the view falls back when the tile's fields cannot be drawn that way", () => {
  const model = tileModel(tile("sparkline", [ref("count")]), apps(shop()), answers(), NOW);
  assert.equal(model.view, "number");
});

test("a tile keeps its catalogue when the app goes away", () => {
  // /api/apps keeps the last good catalogue next to ok: false.
  const failed = answer({ ok: false, error: "cannot reach the app", fetched_at: ago(40) });
  const model = tileModel(tile("number", [ref("count")]), apps(shop({ ok: false, error: "cannot reach the app" })), answers(failed), NOW);
  assert.equal(model.state, "stale");
  assert.equal(model.title, "Orders today");
});
