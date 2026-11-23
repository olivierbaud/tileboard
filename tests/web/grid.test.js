// Run with: node --test "tests/web/*.test.js"   (`cargo test` does it too, when Node is installed)

import assert from "node:assert/strict";
import test from "node:test";

import { COLUMNS, add, clamp, nudge, overlap, pack, place, remove } from "../../web/grid.js";

const tile = (id, x, y, w, h) => ({ id, x, y, w, h });
/** Tiles as "id:x,y wxh", sorted, for easy comparing. */
const show = (tiles) => tiles.map(({ id, x, y, w, h }) => `${id}:${x},${y} ${w}x${h}`).sort();
const noOverlaps = (tiles) => tiles.every((a, i) => tiles.slice(i + 1).every((b) => !overlap(a, b)));

// A small wall:   AAAB
//                 AAAB
//                 CCCC
const wall = () => [tile("a", 0, 0, 3, 2), tile("b", 3, 0, 1, 2), tile("c", 0, 2, 4, 1)];

test("tiles overlap when they share a cell, not when they only touch", () => {
  assert.ok(overlap(tile("a", 0, 0, 2, 2), tile("b", 1, 1, 2, 2)));
  assert.ok(!overlap(tile("a", 0, 0, 2, 2), tile("b", 2, 0, 2, 2)), "side by side");
  assert.ok(!overlap(tile("a", 0, 0, 2, 2), tile("b", 0, 2, 2, 2)), "one above the other");
  assert.ok(overlap(tile("a", 0, 0, 12, 1), tile("b", 5, 0, 1, 1)));
});

test("a tile is kept inside the grid", () => {
  assert.deepEqual(clamp({ x: 3, y: 2, w: 4, h: 2 }), { x: 3, y: 2, w: 4, h: 2 });
  assert.deepEqual(clamp({ x: -2, y: -1, w: 0, h: 0 }), { x: 0, y: 0, w: 1, h: 1 });
  assert.deepEqual(clamp({ x: 10, y: 0, w: 5, h: 1 }), { x: 7, y: 0, w: 5, h: 1 }, "pushed left to fit");
  assert.deepEqual(clamp({ x: 0, y: 0, w: 40, h: 1 }), { x: 0, y: 0, w: COLUMNS, h: 1 });
  assert.deepEqual(clamp({ x: 2.6, y: 1.4, w: 2.5, h: 1.2 }), { x: 3, y: 1, w: 3, h: 1 }, "whole cells");
});

test("packing closes holes and keeps a packed wall as it is", () => {
  assert.deepEqual(show(pack(wall())), show(wall()));
  const holes = [tile("a", 0, 3, 3, 2), tile("b", 3, 1, 1, 2), tile("c", 0, 9, 4, 1)];
  assert.deepEqual(show(pack(holes)), ["a:0,0 3x2", "b:3,0 1x2", "c:0,2 4x1"]);
  assert.deepEqual(pack([]), []);
});

test("packing keeps the order the tiles came in and does not change them", () => {
  const before = [tile("c", 0, 9, 4, 1), tile("a", 0, 3, 3, 2)];
  const copy = structuredClone(before);
  assert.deepEqual(
    pack(before).map(({ id }) => id),
    ["c", "a"],
  );
  assert.deepEqual(before, copy);
});

test("a moved tile takes its place and the others make room", () => {
  // Drop B on the top left corner: A moves down, C below it.
  const moved = place(wall(), "b", { x: 0, y: 0 });
  assert.deepEqual(show(moved), ["a:0,2 3x2", "b:0,0 1x2", "c:0,4 4x1"]);
  assert.ok(noOverlaps(moved));
});

test("a tile dropped far below rises as far as it can", () => {
  // Nothing stands above B's column: it comes all the way back up.
  assert.deepEqual(show(place(wall(), "b", { x: 3, y: 20 })), show(wall()));
  // Dropped under C's columns, it stops below C.
  assert.deepEqual(show(place(wall(), "b", { x: 1, y: 20 })), ["a:0,0 3x2", "b:1,3 1x2", "c:0,2 4x1"]);
});

test("a tile moved aside lets the tiles under it rise", () => {
  // A moves to the right of B: C can come up under where A was, as far as B allows.
  const moved = place(wall(), "a", { x: 4, y: 0 });
  assert.deepEqual(show(moved), ["a:4,0 3x2", "b:3,0 1x2", "c:0,2 4x1"]);
  // B moves away too: now C rises to the top.
  const again = place(moved, "b", { x: 8, y: 0 });
  assert.deepEqual(show(again), ["a:4,0 3x2", "b:8,0 1x2", "c:0,0 4x1"]);
});

test("dragging a tile down by its own height swaps it with the one below", () => {
  const stack = [tile("a", 0, 0, 2, 2), tile("b", 0, 2, 2, 3)];
  assert.deepEqual(show(place(stack, "a", { y: 1 })), show(stack), "not far enough yet");
  assert.deepEqual(show(place(stack, "a", { y: 2 })), ["a:0,3 2x2", "b:0,0 2x3"]);
  // And dragging the lower one up to the top of the upper one swaps them back.
  const swapped = place(stack, "a", { y: 2 });
  assert.deepEqual(show(place(swapped, "a", { y: 1 })), show(swapped), "not far enough yet");
  assert.deepEqual(show(place(swapped, "a", { y: 0 })), show(stack));
});

test("the arrow keys always get a tile somewhere new, if there is anywhere to go", () => {
  const stack = [tile("a", 0, 0, 2, 2), tile("b", 0, 2, 2, 3), tile("c", 2, 0, 2, 1)];
  // Down: one cell changes nothing on a packed wall, so it goes on until A is below B.
  assert.deepEqual(show(nudge(stack, "a", 0, 1)), ["a:0,3 2x2", "b:0,0 2x3", "c:2,0 2x1"]);
  // Up from below: back above B.
  assert.deepEqual(show(nudge(nudge(stack, "a", 0, 1), "a", 0, -1)), show(stack));
  // Right: A takes C's place and C goes under it.
  assert.deepEqual(show(nudge(stack, "a", 1, 0)), ["a:1,0 2x2", "b:0,2 2x3", "c:2,2 2x1"]);
  // Nowhere to go: the same tiles come back.
  assert.equal(nudge(stack, "a", -1, 0), stack, "already at the left edge");
  assert.equal(nudge(stack, "a", 0, -1), stack, "already at the top");
  const alone = [tile("a", 0, 0, 2, 2)];
  assert.equal(nudge(alone, "a", 0, 1), alone, "alone: down leads nowhere");
  assert.equal(nudge(stack, "nope", 1, 0), stack);
});

test("a tile that grows pushes its neighbours down", () => {
  const grown = place(wall(), "a", { w: 4, h: 3 });
  assert.deepEqual(show(grown), ["a:0,0 4x3", "b:3,3 1x2", "c:0,5 4x1"]);
  assert.ok(noOverlaps(grown));
  // And shrinking lets them come back.
  assert.deepEqual(show(place(grown, "a", { w: 3, h: 2 })), show(wall()));
});

test("a move past the edge is kept on the board", () => {
  const moved = place(wall(), "a", { x: 50, y: -3 });
  assert.equal(moved.find(({ id }) => id === "a").x, COLUMNS - 3);
  assert.ok(moved.every(({ x, y, w }) => x >= 0 && y >= 0 && x + w <= COLUMNS));
});

test("taking a tile away closes the gap", () => {
  assert.deepEqual(show(remove(wall(), "a")), ["b:3,0 1x2", "c:0,2 4x1"]);
  assert.deepEqual(show(remove(wall(), "b")), ["a:0,0 3x2", "c:0,2 4x1"]);
  assert.deepEqual(show(remove(remove(wall(), "a"), "b")), ["c:0,0 4x1"]);
  assert.deepEqual(show(remove(wall(), "nope")), show(wall()));
});

test("a new tile goes into the first free place", () => {
  // Next to B on the top row there is room for 8 columns.
  assert.deepEqual(show(add(wall(), tile("n", 0, 0, 3, 2))).at(-1), "n:4,0 3x2");
  // Too wide for that: it goes below C.
  assert.deepEqual(show(add(wall(), tile("n", 0, 0, 12, 1))).at(-1), "n:0,3 12x1");
  // An empty board: top left.
  assert.deepEqual(show(add([], tile("n", 5, 5, 2, 2))), ["n:0,0 2x2"]);
  // A size that does not fit the grid is cut to it.
  assert.deepEqual(show(add([], tile("n", 0, 0, 30, 0))), ["n:0,0 12x1"]);
});

test("whatever is done, tiles never overlap and never leave the board", () => {
  // A fixed, made-up sequence of moves, sizes, removals and additions.
  let tiles = [];
  let seed = 7;
  const next = (below) => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed % below;
  };
  for (let step = 0; step < 300; step += 1) {
    const action = next(10);
    if (tiles.length < 3 || action < 2) {
      tiles = add(tiles, tile(`t${step}`, 0, 0, 1 + next(6), 1 + next(4)));
    } else if (action < 3) {
      tiles = remove(tiles, tiles[next(tiles.length)].id);
    } else if (action < 7) {
      tiles = place(tiles, tiles[next(tiles.length)].id, { x: next(14) - 1, y: next(12) - 1 });
    } else {
      tiles = place(tiles, tiles[next(tiles.length)].id, { w: next(14), h: next(6) });
    }
    assert.ok(noOverlaps(tiles), `overlap after step ${step}`);
    assert.ok(tiles.every(({ x, y, w, h }) => x >= 0 && y >= 0 && w >= 1 && h >= 1 && x + w <= COLUMNS));
    // Packed: no tile could rise by one row.
    for (const one of tiles) {
      const risen = { ...one, y: one.y - 1 };
      const free = one.y > 0 && tiles.every((other) => other.id === one.id || !overlap(risen, other));
      assert.ok(!free, `tile ${one.id} could rise after step ${step}`);
    }
  }
});
