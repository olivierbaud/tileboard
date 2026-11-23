// The page: loads the layout and the catalogues, keeps asking the dashboard
// server for data, and redraws the tiles whose data changed.

import { setUpEditing } from "./edit.js";
import { pollSeconds, sourcesByApp, tileModel } from "./model.js";
import { drawTile, el, statusIcon } from "./views.js";

/** Catalogues are read again this often, or sooner while an app is not reachable. */
const APPS_EVERY_MS = 5 * 60_000;
const APPS_RETRY_MS = 30_000;
/** A request to the dashboard server is given up on after this long. */
const REQUEST_LIMIT_MS = 20_000;

const board = document.getElementById("board");
const statusLine = document.getElementById("status");

const state = {
  /** name → app, as `/api/apps` gives it. Null until the first answer. */
  apps: null,
  layout: { rev: 0, tiles: [] },
  /** source key → answer, as `/api/data` gives it. */
  answers: new Map(),
  /** tile id → { tile, root, memory, drawn }. */
  tiles: new Map(),
  /** The server's clock minus this device's clock. Ages are worked out on the server's clock. */
  clockSkewMs: 0,
  lastAnswerAt: null,
  disconnectedSince: null,
  /** Goes up each time the board is rebuilt; polling loops of an older round stop. */
  round: 0,
  /** While the board is being edited: { saved, dirty }. `layout` is then the unsaved one. */
  editing: null,
};

async function request(path, options = {}) {
  const response = await fetch(path, { ...options, signal: AbortSignal.timeout(REQUEST_LIMIT_MS) });
  if (!response.ok) throw new Error(`${path} answered ${response.status}`);
  return response.json();
}

// ------------------------------------------------------------------ tiles

function draw(entry) {
  const model = tileModel(entry.tile, state.apps, state.answers, Date.now() + state.clockSkewMs);
  // An unchanged tile is left alone, so a text selection or a scrolled list is not disturbed.
  const drawn = JSON.stringify(model);
  if (drawn === entry.drawn) return;
  entry.drawn = drawn;
  drawTile(entry.root, model, entry.memory);
}

function drawAll() {
  for (const entry of state.tiles.values()) draw(entry);
}

function buildBoard() {
  state.round += 1;
  state.tiles.clear();
  // In reading order (top to bottom, then left to right): also the order on a narrow screen.
  const tiles = [...state.layout.tiles].sort((a, b) => a.y - b.y || a.x - b.x);
  const roots = tiles.map((tile) => {
    const root = el("article", "tile");
    for (const side of ["x", "y", "w", "h"]) root.style.setProperty(`--${side}`, tile[side]);
    root.dataset.size = tile.w <= 4 ? "small" : "wide";
    state.tiles.set(tile.id, { tile, root, memory: {}, drawn: "" });
    return root;
  });
  board.replaceChildren(...roots);
  drawEmptyBoard();
  drawAll();
  editing.decorate();

  const round = state.round;
  for (const [app, sources] of sourcesByApp(state.layout)) pollLoop(round, app, sources);
}

/** What the board says when the layout has no tiles: which apps are ready to be shown. */
function drawEmptyBoard() {
  board.querySelector(".empty-board")?.remove();
  board.classList.toggle("is-empty", state.layout.tiles.length === 0);
  if (state.layout.tiles.length > 0) return;

  const box = el("section", "empty-board");
  box.append(el("h2", "", "No tiles yet"));
  const how = state.editing ? "Choose Add tile to make the first one." : "Choose Edit, then Add tile, to make the first one.";
  box.append(el("p", "", `${how} These apps are ready to be shown:`));
  const list = el("ul");
  for (const app of state.apps?.values() ?? []) {
    const item = el("li");
    item.append(statusIcon(app.ok ? "ok" : "crit"));
    const sources = app.catalogue?.sources.map((source) => source.title).join(", ");
    item.append(el("strong", "", app.catalogue?.title ?? app.name));
    item.append(el("span", "", app.ok ? sources : `not reachable: ${app.error}`));
    list.append(item);
  }
  if (state.apps?.size === 0) list.append(el("li", "", "No apps yet. Add one to config.toml."));
  box.append(list);
  board.append(box);
}

// ---------------------------------------------------------------- polling

/** Sleeps that can be cut short, so the page catches up at once when it becomes visible again. */
const wakeUps = new Set();

function sleep(ms) {
  return new Promise((resolve) => {
    const wake = () => {
      clearTimeout(timer);
      wakeUps.delete(wake);
      resolve();
    };
    const timer = setTimeout(wake, ms);
    wakeUps.add(wake);
  });
}

function wakeAll() {
  for (const wake of [...wakeUps]) wake();
}

document.addEventListener("visibilitychange", () => {
  if (!document.hidden) wakeAll();
});

/** Keeps one app's sources up to date. One loop per app, so a slow app does not hold up the others. */
async function pollLoop(round, app, sources) {
  while (round === state.round) {
    if (!document.hidden) await poll(round, app, sources);
    if (round !== state.round) return;
    await sleep(pollSeconds(sources, state.apps?.get(app)) * 1000);
  }
}

async function poll(round, app, sources) {
  let answer;
  try {
    answer = await request("/api/data", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sources: sources.map(({ source, params }) => ({ app, source, params })) }),
    });
  } catch {
    disconnected();
    return;
  }
  if (round !== state.round) return;

  answer.sources.forEach((sourceAnswer, i) => state.answers.set(sources[i].key, sourceAnswer));
  connected(answer.now);
  for (const entry of state.tiles.values()) {
    if (entry.tile.fields.some((field) => field.app === app)) draw(entry);
  }
  // The layout was saved somewhere else: show the new one. (Not while this board is being
  // edited: that would throw the edits away. Saving will ask which layout to keep.)
  if (!state.editing && answer.layout_rev !== state.layout.rev) await loadLayout().catch(disconnected);
}

async function loadLayout() {
  state.layout = await request("/api/layout");
  buildBoard();
}

/** Keeps the app list (the catalogues) up to date. The board does not wait for it. */
async function appsLoop() {
  for (;;) {
    if (!document.hidden) {
      try {
        const { apps } = await request("/api/apps");
        const first = state.apps === null;
        state.apps = new Map(apps.map((app) => [app.name, app]));
        drawEmptyBoard();
        drawAll();
        showStatus();
        // The polling loops guessed their pace without the catalogues: let them look again.
        if (first) wakeAll();
      } catch {
        disconnected();
      }
    }
    const allFine = state.apps !== null && [...state.apps.values()].every((app) => app.ok);
    await sleep(allFine ? APPS_EVERY_MS : APPS_RETRY_MS);
  }
}

// ------------------------------------------------------------- status line

function connected(serverTime) {
  state.clockSkewMs = Date.parse(serverTime) - Date.now();
  state.lastAnswerAt = new Date();
  state.disconnectedSince = null;
  showStatus();
}

function disconnected() {
  state.disconnectedSince ??= state.lastAnswerAt ?? new Date();
  showStatus();
}

function showStatus() {
  const time = (date) => date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  // An app counts as not reachable when its catalogue could not be read, or when
  // a call for its data has just failed (the app list is only re-read every few minutes).
  // (`refresh_s` is only set once a call was really made: a mistake in a tile is not an outage.)
  const failed = [...state.answers.values()].filter((answer) => !answer.ok && answer.refresh_s !== null);
  const failing = new Map(failed.map((answer) => [answer.app, answer.error]));
  const down = [...(state.apps?.values() ?? [])]
    .filter((app) => !app.ok || failing.has(app.name))
    .map((app) => ({ ...app, error: app.error ?? failing.get(app.name) }));
  // While the server does not answer, the tiles keep their last values, dimmed.
  document.body.classList.toggle("disconnected", state.disconnectedSince !== null);

  let icon = null;
  let text = state.lastAnswerAt ? `Updated ${time(state.lastAnswerAt)}` : "";
  let detail = "";
  if (state.disconnectedSince) {
    icon = "crit";
    text = `No answer from the dashboard server since ${time(state.disconnectedSince)}. Showing the last values.`;
  } else if (down.length > 0) {
    icon = "warn";
    const names = down.map((app) => app.catalogue?.title ?? app.name).join(", ");
    text = `Not reachable: ${names}. ${text}`;
    detail = down.map((app) => `${app.name}: ${app.error}`).join("\n");
  }
  statusLine.replaceChildren(...(icon ? [statusIcon(icon)] : []), el("span", "", text));
  statusLine.title = detail;
}

// -------------------------------------------------------------------- start

const editing = setUpEditing({
  state,
  board,
  actions: document.getElementById("actions"),
  rebuild: buildBoard,
  request,
  now: () => Date.now() + state.clockSkewMs,
});

async function start() {
  // The app list can be slow (an app that is down is waited for once), so the board is
  // drawn from the layout alone and fills in as the app list and the data arrive.
  appsLoop();
  for (;;) {
    try {
      state.layout = await request("/api/layout");
      break;
    } catch {
      disconnected();
      await sleep(5000);
    }
  }
  state.disconnectedSince = null;
  showStatus();
  buildBoard();
}

start();
