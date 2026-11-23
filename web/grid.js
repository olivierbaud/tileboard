// The grid the tiles sit on: 12 columns, as many rows as needed. Pure functions.
//
// Tiles never overlap and never leave a hole above them: after every change the
// tiles are packed upward, the way a wall is tiled from the top.

export const COLUMNS = 12;

/** Whether two tiles share at least one cell. */
export function overlap(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

/** A position and size that fit the grid: whole cells, at least 1 x 1, inside the 12 columns. */
export function clamp({ x, y, w, h }) {
  const width = Math.min(COLUMNS, Math.max(1, Math.round(w)));
  return {
    x: Math.min(COLUMNS - width, Math.max(0, Math.round(x))),
    y: Math.max(0, Math.round(y)),
    w: width,
    h: Math.max(1, Math.round(h)),
  };
}

/**
 * Packs the tiles upward: in reading order, each tile rises as far as it can
 * without touching one placed before it. Returns new tiles in the order they came in.
 *
 * The tile named `moved` is the one a person just put somewhere. It goes before
 * the other tiles of its row, so it keeps its place and they make room; unless
 * it came from above (`fromAbove`), when the tiles of that row go first, so that
 * dragging a tile down by its own height swaps it with the one below.
 */
export function pack(tiles, moved = null, fromAbove = false) {
  const rank = (tile) => (tile.id === moved ? tile.y + (fromAbove ? 0.5 : -0.5) : tile.y);
  const order = [...tiles].sort((a, b) => rank(a) - rank(b) || a.x - b.x);
  const placed = new Map();
  for (const tile of order) {
    const next = { ...tile, y: 0 };
    const taken = [...placed.values()];
    while (taken.some((other) => overlap(next, other))) next.y += 1;
    placed.set(tile.id, next);
  }
  return tiles.map((tile) => placed.get(tile.id));
}

/** Moves or resizes one tile (`change` holds any of x, y, w, h); the others make room. */
export function place(tiles, id, change) {
  const before = tiles.find((tile) => tile.id === id);
  if (!before) return tiles;
  const after = { ...before, ...clamp({ ...before, ...change }) };
  return pack(
    tiles.map((tile) => (tile === before ? after : tile)),
    id,
    after.y > before.y,
  );
}

/**
 * Moves a tile one step in a direction (`dx`, `dy` are -1, 0 or 1), for the arrow keys.
 * On a packed wall one cell is often not enough to change anything, so it goes on
 * in that direction until the tile lands somewhere new. Returns `tiles` itself if it cannot.
 */
export function nudge(tiles, id, dx, dy) {
  const tile = tiles.find((candidate) => candidate.id === id);
  if (!tile) return tiles;
  for (let steps = 1; steps <= 100; steps += 1) {
    const x = tile.x + dx * steps;
    if (x < 0 || x + tile.w > COLUMNS) break;
    const moved = place(tiles, id, { x, y: tile.y + dy * steps });
    const now = moved.find((candidate) => candidate.id === id);
    if (now.x !== tile.x || now.y !== tile.y) return moved;
  }
  return tiles;
}

/** Takes one tile away; the others close the gap. */
export function remove(tiles, id) {
  return pack(tiles.filter((tile) => tile.id !== id));
}

/** Adds a tile in the first free place, reading left to right and top to bottom. */
export function add(tiles, tile) {
  const size = clamp({ x: 0, y: 0, w: tile.w, h: tile.h });
  for (let y = 0; ; y += 1) {
    for (let x = 0; x <= COLUMNS - size.w; x += 1) {
      const candidate = { ...tile, ...size, x, y };
      if (!tiles.some((other) => overlap(candidate, other))) return [...tiles, candidate];
    }
  }
}
