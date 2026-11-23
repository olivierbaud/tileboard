// Drawing tiles. Everything that comes from an app goes on the page as text
// (textContent), never as HTML.

import { NO_DATA, STATUS_WORDS, columnDigits, entryText, formatNumber, itemText, numberParts } from "./format.js";

const SVG = "http://www.w3.org/2000/svg";

export function el(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function svgEl(tag, attributes = {}) {
  const node = document.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attributes)) node.setAttribute(name, value);
  return node;
}

// ------------------------------------------------------------------ status

/**
 * A status mark. The shape says it without colour: a circle with a tick (ok),
 * a triangle (warning), a diamond with a cross (critical), an empty ring (no status).
 */
export function statusIcon(status) {
  const icon = svgEl("svg", { viewBox: "0 0 16 16", class: `icon icon-${status ?? "none"}`, "aria-hidden": "true" });
  if (status === "ok") {
    icon.append(svgEl("circle", { cx: 8, cy: 8, r: 7, class: "fill" }));
    icon.append(svgEl("path", { d: "M4.7 8.4l2.2 2.2 4.4-4.8", class: "mark" }));
  } else if (status === "warn") {
    icon.append(svgEl("path", { d: "M8 2 14.6 13.6H1.4Z", class: "fill" }));
    icon.append(svgEl("path", { d: "M8 6.4v3.2M8 11.7v.1", class: "mark" }));
  } else if (status === "crit") {
    icon.append(svgEl("path", { d: "M8 1.4 14.6 8 8 14.6 1.4 8Z", class: "fill" }));
    icon.append(svgEl("path", { d: "M6 6l4 4M10 6l-4 4", class: "mark" }));
  } else {
    icon.append(svgEl("circle", { cx: 8, cy: 8, r: 5.6, class: "ring" }));
  }
  return icon;
}

/** A status as mark + word. */
function statusChip(status) {
  const chip = el("span", "chip");
  chip.dataset.status = status;
  chip.append(statusIcon(status), el("span", "", STATUS_WORDS[status]));
  return chip;
}

// ------------------------------------------------------------------ pieces

/** A value set large: the number in ink, its unit smaller beside it. */
function figure({ spec, entry }, className = "figure") {
  const node = el("div", className);
  const value = entry.value;
  if (value === null || value === undefined) {
    node.classList.add("empty");
    node.textContent = NO_DATA;
    if (entry.error) node.title = `The app sent a value that does not fit: ${entry.error}`;
    node.style.setProperty("--chars", 2);
    return node;
  }
  if (spec.type === "number" || spec.type === "series") {
    const { prefix, number, suffix } = numberParts(value, spec.unit, { compact: true });
    if (prefix) node.append(el("span", "unit before", prefix));
    node.append(el("span", "number", number));
    if (suffix) node.append(el("span", "unit", suffix));
    node.title = formatNumber(value, spec.unit);
    // Units are set at half size, so they count for half when fitting the figure to the tile.
    node.style.setProperty("--chars", Math.max(2, number.length + (prefix.length + suffix.length) / 2));
    return node;
  }
  const text = entryText(spec, entry);
  node.classList.add("words");
  node.textContent = text;
  node.style.setProperty("--chars", Math.max(4, Math.min(text.length, 18)));
  return node;
}

/** A row: label on the left, value on the right, a status mark before the value if there is one. */
function row(label, value, status, title) {
  const item = el("li", "row");
  item.append(el("span", "label", label));
  const right = el("span", "value");
  if (status) {
    right.append(statusIcon(status));
    // The mark is a picture: say the status in words too, for whoever cannot see it.
    if (value !== STATUS_WORDS[status]) right.append(el("span", "unseen", `${STATUS_WORDS[status]}: `));
  }
  right.append(el("span", "", value));
  if (title) right.title = title;
  item.append(right);
  return item;
}

function fieldRow({ spec, entry }) {
  // A status field says its status in words already; other fields show it as a mark.
  const status = spec.type === "status" ? entry.value : entry.status;
  return row(spec.label, entryText(spec, entry), status ?? null, entry.error);
}

// ------------------------------------------------------------------- views

function numberView(fields) {
  const [first, ...rest] = fields;
  const wrap = el("div", "figures");
  if (first.spec.type === "status") {
    wrap.append(light(first, false));
  } else {
    wrap.append(figure(first));
    // With several fields on the tile, say which one the big value is.
    if (rest.length > 0) wrap.append(el("p", "caption", first.spec.label));
    if (first.entry.status) wrap.append(statusChip(first.entry.status));
  }
  if (rest.length > 0) {
    const more = el("ul", "rows");
    more.append(...rest.map(fieldRow));
    wrap.append(more);
  }
  return wrap;
}

function tableView(fields) {
  const rows = el("ul", "rows fill");
  rows.append(...fields.map(fieldRow));
  return rows;
}

function listView(fields) {
  const rows = el("ul", "rows fill");
  for (const field of fields) {
    const { spec, entry } = field;
    if (spec.type !== "list") {
      rows.append(fieldRow(field));
      continue;
    }
    // With several fields on the tile, name each list.
    if (fields.length > 1) rows.append(el("li", "group", spec.label));
    if (!Array.isArray(entry.value)) {
      rows.append(row(fields.length > 1 ? "" : spec.label, NO_DATA, null, entry.error));
    } else if (entry.value.length === 0) {
      rows.append(el("li", "row quiet", "Nothing in this list right now"));
    } else {
      // One number of decimals for the whole column, so the amounts line up.
      const digits = columnDigits(
        entry.value.map((item) => item.value),
        spec.unit,
      );
      for (const item of entry.value) {
        const text = itemText(item, spec.unit, { digits }) || (item.status ? STATUS_WORDS[item.status] : "");
        rows.append(row(item.label, text, item.status ?? null));
      }
    }
  }
  return rows;
}

/** One status light: a big mark, the state in words, and the field's name if `named`. */
function light({ spec, entry }, named) {
  const status = spec.type === "status" ? entry.value : entry.status;
  const node = el("div", "light");
  node.append(statusIcon(status ?? null));
  const words = el("div", "words");
  const state = spec.type === "status" ? (STATUS_WORDS[entry.value] ?? "No data") : entryText(spec, entry);
  words.append(el("strong", "", state));
  if (named) words.append(el("span", "", spec.label));
  node.append(words);
  if (entry.error) node.title = entry.error;
  return node;
}

function statusView(fields) {
  const lights = el("div", "lights");
  lights.append(...fields.map((field) => light(field, fields.length > 1)));
  return lights;
}

function sparklineView(fields, memory) {
  const chart = fields.find(({ spec }) => spec.type === "series");
  const rest = fields.filter((field) => field !== chart);
  const wrap = el("div", "trend");

  const top = el("div", "trend-top");
  top.append(figure(chart, "figure small"));
  if (chart.entry.status) top.append(statusChip(chart.entry.status));
  wrap.append(top);
  if (rest.length > 0) {
    const more = el("ul", "rows");
    more.append(...rest.map(fieldRow));
    wrap.append(more);
  }
  wrap.append(sparkline(chart, memory));
  return wrap;
}

// --------------------------------------------------------------- sparkline

/** The sparkline whose readout is showing, if any: { root, hide }. */
let showing = null;

if (typeof document !== "undefined") {
  // A tile is redrawn under a resting pointer without a "leave" event, so the
  // readout is also hidden as soon as the pointer is seen anywhere else.
  document.addEventListener("pointermove", (event) => {
    if (showing && !showing.pinned && !showing.root.contains(event.target)) showing.hide();
  });
}

/**
 * A small line chart of a series field. Hovering (or focusing it and pressing the
 * arrow keys) reads out single points. `memory` survives redraws, so the readout
 * stays where it was when new data arrives.
 */
function sparkline({ spec, entry }, memory) {
  const points = Array.isArray(entry.series) ? entry.series : [];
  const known = points.map((value, index) => ({ value, index })).filter((point) => point.value !== null);

  const root = el("div", "spark");
  if (known.length < 2) {
    root.append(el("p", "quiet", "Not enough points to draw yet"));
    return root;
  }

  const text = (value) => formatNumber(value, spec.unit);
  const values = known.map((point) => point.value);
  let low = Math.min(...values);
  let high = Math.max(...values);
  root.tabIndex = 0;
  root.setAttribute("role", "img");
  root.setAttribute(
    "aria-label",
    `${spec.label}: latest ${text(values.at(-1))}, lowest ${text(low)}, highest ${text(high)}, ${known.length} points. ` +
      "Use the arrow keys to read single points.",
  );

  // Leave some air above and below the line. A flat line sits in the middle.
  const span = high - low || Math.abs(high) * 0.1 || 1;
  low -= span * 0.14;
  high += span * 0.14;
  const x = (index) => (points.length === 1 ? 50 : (index / (points.length - 1)) * 100);
  const y = (value) => (1 - (value - low) / (high - low)) * 100;

  const plot = el("div", "plot");
  const svg = svgEl("svg", { viewBox: "0 0 100 100", preserveAspectRatio: "none", "aria-hidden": "true" });

  // One line (and its wash) per unbroken run of points; a null is a gap.
  let run = [];
  const flush = () => {
    if (run.length > 1) {
      const line = run.map((point, i) => `${i === 0 ? "M" : "L"}${x(point.index).toFixed(2)} ${y(point.value).toFixed(2)}`);
      const first = x(run[0].index).toFixed(2);
      const last = x(run.at(-1).index).toFixed(2);
      svg.append(svgEl("path", { d: `M${first} 100 ${line.join(" ").replace("M", "L")} L${last} 100Z`, class: "wash" }));
      svg.append(svgEl("path", { d: line.join(" "), class: "line" }));
    }
    run = [];
  };
  points.forEach((value, index) => {
    if (value === null) flush();
    else run.push({ value, index });
  });
  flush();
  plot.append(svg);

  // The field's thresholds, where they fall inside the picture. With two of them,
  // the upper one is labelled above its line and the lower one below.
  const limits = [["warn", spec.warn], ["crit", spec.crit]].filter(
    ([, limit]) => typeof limit === "number" && limit > low && limit < high,
  );
  const lowest = limits.length === 2 ? Math.min(limits[0][1], limits[1][1]) : null;
  for (const [kind, limit] of limits) {
    const rule = el("div", `limit ${kind}${limit === lowest ? " below" : ""}`);
    rule.style.top = `${y(limit)}%`;
    const label = el("span");
    label.append(statusIcon(kind), el("span", "", text(limit)));
    rule.append(label);
    plot.append(rule);
  }

  const place = (node, point) => {
    node.style.left = `${x(point.index)}%`;
    node.style.top = `${y(point.value)}%`;
  };
  const end = el("div", "dot");
  place(end, known.at(-1));
  plot.append(end);

  // The readout: a hairline, a dot and a small label that follow the pointer or the arrow keys.
  const cross = el("div", "cross");
  const marker = el("div", "dot reading");
  const tip = el("div", "tip");
  const tipValue = el("strong");
  const tipWhen = el("span");
  tip.append(tipValue, tipWhen);
  plot.append(cross, marker, tip);

  const me = { root, pinned: false, hide };
  function show(position) {
    // The nearest point that has a value.
    const point = known.reduce((best, candidate) =>
      Math.abs(candidate.index - position) < Math.abs(best.index - position) ? candidate : best,
    );
    memory.fromEnd = points.length - 1 - point.index;
    memory.active = true;
    if (showing && showing !== me) showing.hide();
    showing = me;

    cross.style.left = `${x(point.index)}%`;
    place(marker, point);
    tipValue.textContent = text(point.value);
    tipWhen.textContent = memory.fromEnd === 0 ? "latest" : `${memory.fromEnd} before latest`;
    // Slide the label so it never leaves the tile: left-aligned at the left edge, right-aligned at the right.
    tip.style.left = `${x(point.index)}%`;
    tip.style.transform = `translateX(-${x(point.index)}%)`;
    root.classList.add("reading");
  }
  function hide() {
    memory.active = false;
    root.classList.remove("reading");
    if (showing === me) showing = null;
  }

  plot.addEventListener("pointermove", (event) => {
    const box = plot.getBoundingClientRect();
    show(Math.round(((event.clientX - box.left) / box.width) * (points.length - 1)));
  });
  plot.addEventListener("pointerleave", () => {
    if (!me.pinned) hide();
  });
  // With the keyboard: focus shows the latest point, the arrow keys walk along the line.
  root.addEventListener("focus", () => {
    me.pinned = true;
    show(points.length - 1 - (memory.fromEnd ?? 0));
  });
  root.addEventListener("blur", () => {
    me.pinned = false;
    hide();
  });
  root.addEventListener("keydown", (event) => {
    const step = { ArrowLeft: -1, ArrowRight: 1 }[event.key];
    if (!step) return;
    event.preventDefault();
    const at = known.findIndex((point) => point.index === points.length - 1 - (memory.fromEnd ?? 0));
    show(known[Math.min(known.length - 1, Math.max(0, at + step))].index);
  });

  // New data arrived while a point was being read: keep reading the same place.
  if (memory.active) show(points.length - 1 - (memory.fromEnd ?? 0));

  root.append(plot);
  return root;
}

// -------------------------------------------------------------------- tile

const VIEWS = { number: numberView, table: tableView, list: listView, status: statusView, sparkline: sparklineView };

/**
 * Draws a tile into `root` from its model (see `tileModel`). `memory` is an object
 * the caller keeps per tile; it carries what must survive a redraw.
 */
export function drawTile(root, model, memory) {
  const hadFocus = root.contains(document.activeElement) && document.activeElement.classList.contains("spark");

  root.dataset.view = model.view;
  root.dataset.state = model.state;
  root.dataset.status = model.status ?? "";

  const head = el("header", "head");
  const title = el("h2", "title");
  if (model.link) {
    const link = el("a", "", model.title);
    link.href = model.link;
    link.target = "_blank";
    link.rel = "noopener noreferrer";
    link.title = "Open in the app";
    title.append(link);
  } else {
    title.textContent = model.title;
  }
  head.append(title);
  if (model.note) {
    const note = el("p", "note", model.note.text);
    note.title = model.note.detail;
    head.append(note);
  }

  const body = el("div", "body");
  if (model.fields.length > 0) body.append(VIEWS[model.view](model.fields, memory));
  // What could not be drawn, and why.
  for (const problem of model.problems) body.append(el("p", "problem", problem));

  root.replaceChildren(head, body);
  if (hadFocus) root.querySelector(".spark")?.focus({ preventScroll: true });
}
