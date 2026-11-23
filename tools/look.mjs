// Looks at a running dashboard the way a person would, and reports what it saw.
//
//   node tools/look.mjs [url] [folder for the pictures]
//
// Start the dashboard first (for example `cargo run -- --demo`). It takes pictures
// (desktop dark and light, phone), reads a point off the first sparkline with the
// pointer and then the keyboard, and lists anything the page logged as an error.

import { mkdirSync } from "node:fs";
import { join } from "node:path";

import { openBrowser } from "./browser.mjs";

const url = process.argv[2] ?? "http://127.0.0.1:4700/";
const folder = process.argv[3] ?? "shots";
mkdirSync(folder, { recursive: true });

const browser = await openBrowser();
const report = [];
const say = (line) => {
  report.push(line);
  console.log(line);
};

try {
  for (const [name, look] of [
    ["desktop-dark", { width: 1360, height: 800, scheme: "dark" }],
    ["desktop-light", { width: 1360, height: 800, scheme: "light" }],
    ["phone-light", { width: 390, height: 844, scheme: "light" }],
  ]) {
    await browser.look(look);
    await browser.goto(url);
    say(`picture: ${await browser.shot(join(folder, `${name}.png`), { full: true })}`);
  }

  // What the board holds.
  await browser.look({ width: 1360, height: 800, scheme: "light" });
  await browser.goto(url);
  const tiles = await browser.evaluate(`[...document.querySelectorAll(".tile")].map((tile) => ({
    title: tile.querySelector(".title")?.textContent,
    view: tile.dataset.view,
    state: tile.dataset.state,
    status: tile.dataset.status,
    text: tile.querySelector(".body")?.innerText.replace(/\\s+/g, " ").slice(0, 70),
  }))`);
  say(`tiles: ${tiles.length}`);
  for (const tile of tiles) say(`  ${tile.title} [${tile.view}, ${tile.state}${tile.status ? `, ${tile.status}` : ""}]: ${tile.text}`);
  say(`status line: ${await browser.evaluate(`document.getElementById("status").innerText`)}`);

  // Read a point off the first sparkline: with the pointer, then with the keyboard.
  const plot = await browser.evaluate(`(() => {
    const box = document.querySelector(".plot")?.getBoundingClientRect();
    return box && { x: box.x, y: box.y, width: box.width, height: box.height };
  })()`);
  if (plot) {
    const readout = `(() => {
      const spark = document.querySelector(".spark");
      return spark.classList.contains("reading") ? spark.querySelector(".tip").innerText.replace(/\\s+/g, " ") : "(hidden)";
    })()`;
    await browser.moveTo(plot.x + plot.width * 0.5, plot.y + plot.height * 0.5);
    say(`sparkline, pointer in the middle: ${await browser.evaluate(readout)}`);
    await browser.moveTo(plot.x + plot.width - 1, plot.y + plot.height * 0.5);
    say(`sparkline, pointer at the right edge: ${await browser.evaluate(readout)}`);
    await browser.shot(join(folder, "sparkline-reading.png"));
    await browser.moveTo(5, 5);
    say(`sparkline, pointer gone: ${await browser.evaluate(readout)}`);

    await browser.evaluate(`document.querySelector(".spark").focus()`);
    say(`sparkline, focused: ${await browser.evaluate(readout)}`);
    await browser.press("ArrowLeft");
    await browser.press("ArrowLeft");
    say(`sparkline, two steps left: ${await browser.evaluate(readout)}`);
    await browser.evaluate(`document.activeElement.blur()`);
    say(`sparkline, focus gone: ${await browser.evaluate(readout)}`);
  }

  // Nothing may stick out of its tile.
  const spills = await browser.evaluate(`[...document.querySelectorAll(".tile")].flatMap((tile) => {
    const box = tile.getBoundingClientRect();
    return [...tile.querySelectorAll(".figure, .title, .chip, .light, .trend-top")]
      .filter((node) => {
        const inner = node.getBoundingClientRect();
        return inner.right > box.right + 0.5 || inner.bottom > box.bottom + 0.5 || inner.left < box.left - 0.5;
      })
      .map((node) => tile.querySelector(".title").textContent + ": " + node.className);
  })`);
  say(spills.length === 0 ? "nothing sticks out of its tile" : `STICKS OUT: ${spills.join("; ")}`);

  say(browser.problems.length === 0 ? "the page logged no errors" : `PAGE ERRORS:\n  ${browser.problems.join("\n  ")}`);
} finally {
  await browser.close();
}
