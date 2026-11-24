// Edit mode: every tile can be dragged and resized, tiles can be added, changed
// and removed. Done saves the layout and locks the board again; Cancel puts
// everything back as it was.

import { COLUMNS, add, nudge, place, remove } from "./grid.js";
import { startSize } from "./model.js";
import { openPicker } from "./picker.js";
import { el } from "./views.js";

const SVG = "http://www.w3.org/2000/svg";
const SIDES = ["x", "y", "w", "h"];
/** Under this width the board is two columns, and tiles are not dragged or resized. */
const NARROW = "(max-width: 720px)";

function button(text, className = "") {
  const node = el("button", className, text);
  node.type = "button";
  return node;
}

/** A small picture button: `path` is drawn in a 16 x 16 box, `name` is what it says to a screen reader. */
function iconButton(path, name) {
  const node = button("", "tool");
  node.setAttribute("aria-label", name);
  node.title = name;
  const icon = document.createElementNS(SVG, "svg");
  icon.setAttribute("viewBox", "0 0 16 16");
  icon.setAttribute("aria-hidden", "true");
  const line = document.createElementNS(SVG, "path");
  line.setAttribute("d", path);
  icon.append(line);
  node.append(icon);
  return node;
}

/** A question with a button per answer. Resolves to the chosen answer's key, or null if dismissed. */
function ask(question, answers) {
  return new Promise((resolve) => {
    const dialog = el("dialog", "ask");
    dialog.append(el("p", "", question));
    const row = el("div", "ask-row");
    let chosen = null;
    answers.forEach(([key, text], i) => {
      const choice = button(text, i === 0 ? "primary" : "");
      choice.addEventListener("click", () => {
        chosen = key;
        dialog.close();
      });
      row.append(choice);
    });
    dialog.append(row);
    dialog.addEventListener("close", () => {
      dialog.remove();
      resolve(chosen);
    });
    document.body.append(dialog);
    dialog.showModal();
  });
}

/**
 * Sets up edit mode. `context` is what the page shares with it:
 * { state, board, actions, rebuild(), request(path, options), now() }.
 * Returns { decorate() }, which the page calls each time it has rebuilt the board.
 */
export function setUpEditing(context) {
  const { state, board, actions } = context;

  const editButton = button("Edit");
  const addButton = button("Add tile");
  const cancelButton = button("Cancel");
  const doneButton = button("Done", "primary");
  const hint = el("p", "edit-hint");
  hint.setAttribute("role", "status");

  const narrow = () => matchMedia(NARROW).matches;
  const say = (text) => {
    hint.textContent = text;
  };
  const usualHint = () =>
    say(
      narrow()
        ? "Tiles can be added, changed and removed here. Moving and resizing needs a wider screen."
        : "Drag a tile to move it. Pull its corner to resize it.",
    );

  function showActions() {
    if (state.editing) actions.replaceChildren(hint, addButton, cancelButton, doneButton);
    else actions.replaceChildren(...(context.viewButtons ?? []), editButton);
  }

  function enter() {
    state.editing = { saved: structuredClone(state.layout), dirty: false };
    document.body.classList.add("editing");
    usualHint();
    showActions();
    context.rebuild();
    addButton.focus();
  }

  function leave(layout) {
    state.editing = null;
    state.layout = layout;
    document.body.classList.remove("editing");
    showActions();
    context.rebuild();
    editButton.focus();
  }

  const changed = () => {
    state.editing.dirty = true;
  };

  async function save() {
    doneButton.disabled = true;
    doneButton.textContent = "Saving…";
    try {
      const response = await fetch("/api/layout", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(state.layout),
      });
      const body = await response.json().catch(() => ({}));
      if (response.ok) {
        leave(body);
      } else if (response.status === 409) {
        const choice = await ask("The layout was changed on another screen while you were editing.", [
          ["mine", "Keep my changes"],
          ["theirs", "Use the other layout"],
        ]);
        if (choice === "mine") {
          state.layout.rev = body.rev;
          await save();
        } else if (choice === "theirs") {
          leave(await context.request("/api/layout"));
        }
      } else {
        say(`Not saved. ${[body.error, ...(body.problems ?? [])].filter(Boolean).join(". ")}.`);
      }
    } catch {
      say("Not saved: the dashboard server did not answer. Your changes are still here. Try Done again.");
    } finally {
      doneButton.disabled = false;
      doneButton.textContent = "Done";
    }
  }

  editButton.addEventListener("click", enter);
  cancelButton.addEventListener("click", () => leave(state.editing.saved));
  doneButton.addEventListener("click", save);
  addButton.addEventListener("click", () => changeContent(null));
  window.addEventListener("beforeunload", (event) => {
    if (state.editing?.dirty) event.preventDefault();
  });

  // ------------------------------------------------------------ positions

  const positions = () => state.layout.tiles.map(({ id, x, y, w, h }) => ({ id, x, y, w, h }));

  function setBox(node, box) {
    for (const side of SIDES) node.style.setProperty(`--${side}`, box[side]);
  }

  /** Puts tiles where `boxes` says, on the page and in the layout being edited. */
  function apply(boxes) {
    for (const box of boxes) {
      const entry = state.tiles.get(box.id);
      if (!entry) continue;
      for (const side of SIDES) entry.tile[side] = box[side];
      setBox(entry.root, box);
      entry.root.dataset.size = box.w <= 4 ? "small" : "wide";
    }
  }

  /** The size of one grid step on the screen right now. */
  function steps() {
    const style = getComputedStyle(board);
    const gap = parseFloat(style.columnGap);
    const inner = board.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight);
    return { x: (inner - gap * (COLUMNS - 1)) / COLUMNS + gap, y: parseFloat(style.gridAutoRows) + gap };
  }

  /**
   * Follows the pointer from `event` until it is let go. `onMove(dx, dy)` gets how far
   * it has gone; `onEnd(kept)` says whether the move stands (false after Escape).
   */
  function follow(event, node, onMove, onEnd) {
    const start = { x: event.clientX, y: event.clientY };
    node.setPointerCapture(event.pointerId);
    const move = (e) => onMove(e.clientX - start.x, e.clientY - start.y);
    const end = (kept) => {
      node.removeEventListener("pointermove", move);
      node.removeEventListener("pointerup", up);
      node.removeEventListener("pointercancel", cancel);
      window.removeEventListener("keydown", escape);
      if (node.hasPointerCapture(event.pointerId)) node.releasePointerCapture(event.pointerId);
      onEnd(kept);
    };
    const up = () => end(true);
    const cancel = () => end(false);
    const escape = (e) => {
      if (e.key === "Escape") end(false);
    };
    node.addEventListener("pointermove", move);
    node.addEventListener("pointerup", up);
    node.addEventListener("pointercancel", cancel);
    window.addEventListener("keydown", escape);
  }

  function startDrag(event, { root, tile }) {
    if (event.button !== 0 || event.target.closest(".tools, .grip") || narrow()) return;
    event.preventDefault();
    root.focus({ preventScroll: true });
    const step = steps();
    const before = positions();
    const origin = before.find((box) => box.id === tile.id);
    let latest = before;
    let moving = false;
    const ghost = el("div", "ghost");

    follow(
      event,
      root,
      (dx, dy) => {
        // A few pixels are a click, not a drag.
        if (!moving && Math.hypot(dx, dy) < 4) return;
        if (!moving) {
          moving = true;
          root.classList.add("dragging");
          setBox(ghost, origin);
          board.append(ghost);
        }
        root.style.transform = `translate(${dx}px, ${dy}px)`;
        latest = place(before, tile.id, { x: origin.x + Math.round(dx / step.x), y: origin.y + Math.round(dy / step.y) });
        // The other tiles make room at once. The dragged one follows the pointer; its ghost shows where it will land.
        apply(latest.filter((box) => box.id !== tile.id));
        setBox(ghost, latest.find((box) => box.id === tile.id));
      },
      (kept) => {
        ghost.remove();
        root.classList.remove("dragging");
        root.style.transform = "";
        if (!moving) return;
        apply(kept ? latest : before);
        if (kept) changed();
      },
    );
  }

  function startResize(event, { root, tile }) {
    if (event.button !== 0 || narrow()) return;
    event.preventDefault();
    event.stopPropagation();
    const step = steps();
    const before = positions();
    const origin = before.find((box) => box.id === tile.id);
    let latest = before;
    root.classList.add("resizing");

    follow(
      event,
      event.currentTarget,
      (dx, dy) => {
        const w = Math.min(COLUMNS - origin.x, Math.max(1, origin.w + Math.round(dx / step.x)));
        const h = Math.max(1, origin.h + Math.round(dy / step.y));
        latest = place(before, tile.id, { w, h });
        apply(latest);
      },
      (kept) => {
        root.classList.remove("resizing");
        apply(kept ? latest : before);
        if (kept && latest !== before) changed();
      },
    );
  }

  function onKey(event, entry) {
    if (event.target !== entry.root) return;
    const { tile } = entry;
    const arrow = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }[event.key];
    if (arrow && !narrow()) {
      event.preventDefault();
      const [dx, dy] = arrow;
      const next = event.shiftKey
        ? place(positions(), tile.id, {
            w: Math.min(COLUMNS - tile.x, Math.max(1, tile.w + dx)),
            h: Math.max(1, tile.h + dy),
          })
        : nudge(positions(), tile.id, dx, dy);
      apply(next);
      changed();
      say(`${nameOf(entry)}: column ${tile.x + 1}, row ${tile.y + 1}, ${tile.w} wide, ${tile.h} high.`);
    } else if (event.key === "Delete" || event.key === "Backspace") {
      event.preventDefault();
      removeTile(tile.id);
    } else if (event.key === "Enter") {
      event.preventDefault();
      changeContent(tile.id);
    }
  }

  // ------------------------------------------------------- add, change, remove

  const nameOf = (entry) => entry.root.querySelector(".title")?.textContent || "Tile";

  function newId() {
    for (;;) {
      const id = `t${[...crypto.getRandomValues(new Uint8Array(5))].map((byte) => byte.toString(36)).join("")}`;
      if (!state.layout.tiles.some((tile) => tile.id === id)) return id;
    }
  }

  function removeTile(id) {
    const name = nameOf(state.tiles.get(id));
    state.layout.tiles = remove(state.layout.tiles, id);
    changed();
    context.rebuild();
    say(`${name} removed. Cancel brings it back.`);
    addButton.focus();
  }

  /** Opens the picker for a tile (or for a new one, with `id` null) and takes what was chosen. */
  async function changeContent(id) {
    if (!state.apps) {
      say("The list of apps has not arrived yet. Try again in a moment.");
      return;
    }
    const existing = id === null ? null : state.layout.tiles.find((tile) => tile.id === id);
    const chosen = await openPicker(
      {
        apps: state.apps,
        now: context.now,
        getData: async (sources) =>
          (
            await context.request("/api/data", {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify({ sources }),
            })
          ).sources,
      },
      existing,
    );
    if (!chosen) return;

    let focusId = id;
    if (existing) {
      Object.assign(existing, chosen);
    } else {
      focusId = newId();
      const size = startSize(chosen.view, chosen.fields.length);
      const tile = { id: focusId, label: chosen.label, view: chosen.view, x: 0, y: 0, ...size, fields: chosen.fields };
      state.layout.tiles = add(state.layout.tiles, tile);
    }
    changed();
    context.rebuild();
    usualHint();
    const root = state.tiles.get(focusId)?.root;
    root?.scrollIntoView({ block: "nearest" });
    root?.focus({ preventScroll: true });
  }

  // ------------------------------------------------------------- per tile

  /** Gives every tile its edit controls. Called after each rebuild of the board. */
  function decorate() {
    if (!state.editing) return;
    for (const entry of state.tiles.values()) {
      const { root, tile } = entry;
      const name = nameOf(entry);
      root.tabIndex = 0;
      root.setAttribute(
        "aria-label",
        `${name}. Arrow keys move it, Shift with arrow keys resizes it, Enter changes what it shows, Delete removes it.`,
      );

      const tools = el("div", "tools");
      const change = iconButton("M2.5 13.5l.6-3L10.6 3l2.4 2.4-7.5 7.5zM9.2 4.4l2.4 2.4", `Change what ${name} shows`);
      const out = iconButton("M4 4l8 8M12 4l-8 8", `Remove ${name}`);
      change.addEventListener("click", () => changeContent(tile.id));
      out.addEventListener("click", () => removeTile(tile.id));
      tools.append(change, out);

      const grip = el("div", "grip");
      grip.title = "Pull to resize";
      grip.addEventListener("pointerdown", (event) => startResize(event, entry));

      root.append(tools, grip);
      root.addEventListener("pointerdown", (event) => startDrag(event, entry));
      root.addEventListener("keydown", (event) => onKey(event, entry));
    }
  }

  showActions();
  return { decorate };
}
