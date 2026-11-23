// Uses edit mode the way a person would, in a real browser, and checks what got saved.
//
//   node tools/try-edit.mjs [url] [folder for the pictures]
//
// Needs a freshly started `tileboard --demo` (the starter layout, nothing saved yet).
// Drags, resizes, uses the keyboard, removes, adds (a suggested tile and a built one),
// changes a tile, cancels, saves, and saves over a layout changed on "another screen".
// Exits with 1 if anything is not as expected.

import { mkdirSync } from "node:fs";
import { join } from "node:path";

import { openBrowser } from "./browser.mjs";

const url = process.argv[2] ?? "http://127.0.0.1:4700/";
const folder = process.argv[3] ?? "shots";
mkdirSync(folder, { recursive: true });

let failures = 0;
function check(what, ok, detail = "") {
  if (!ok) failures += 1;
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`);
}

const browser = await openBrowser({ port: 9336 });
const savedLayout = async () => (await fetch(new URL("/api/layout", url))).json();
const tile = (title) => `[...document.querySelectorAll(".tile")].find((t) => t.querySelector(".title")?.textContent === ${JSON.stringify(title)})`;
const buttonNamed = (text, within = "document") =>
  `[...${within}.querySelectorAll("button")].find((b) => b.textContent.trim() === ${JSON.stringify(text)})`;
const box = async (title) =>
  browser.evaluate(`(() => {
    const t = ${tile(title)};
    if (!t) return null;
    const s = t.style;
    return ["x", "y", "w", "h"].map((side) => Number(s.getPropertyValue("--" + side))).join(",");
  })()`);
const overlaps = (tiles) =>
  tiles.some((a, i) => tiles.slice(i + 1).some((b) => a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h));
/** One grid step on the screen, from the first tile (3 columns wide, 2 rows high). */
const stepOf = async () => {
  const cpu = await browser.middleOf(tile("CPU"));
  return { x: (cpu.width + 6) / 3, y: (cpu.height + 6) / 2 };
};

try {
  await browser.look({ width: 1360, height: 900, scheme: "light" });
  await browser.goto(url, 1800);
  const before = await savedLayout();
  check("starts from the starter layout", before.rev === 0 && before.tiles.length === 11, `rev ${before.rev}, ${before.tiles.length} tiles`);

  // ---------------------------------------------------------- cancel
  await browser.clickOn(buttonNamed("Edit"));
  check("Edit shows Add tile, Cancel and Done", (await browser.middleOf(buttonNamed("Done"))) !== null);
  const step = await stepOf();
  const cpu = await browser.middleOf(tile("CPU"));
  await browser.drag(cpu, [
    { x: cpu.x + 40, y: cpu.y + 5 },
    { x: cpu.x + step.x * 5, y: cpu.y },
  ]);
  check("dragging CPU 5 columns right moves it", (await box("CPU")) === "5,0,3,2", await box("CPU"));
  await browser.clickOn(buttonNamed("Cancel"));
  check("Cancel puts it back", (await box("CPU")) === "0,0,3,2", await box("CPU"));
  check("Cancel saves nothing", (await savedLayout()).rev === 0);

  // ------------------------------------------------------- move and resize
  await browser.clickOn(buttonNamed("Edit"));
  await browser.shot(join(folder, "edit-mode.png"), { full: true });

  // Drag "Last backup" (8,0 2x2) to the far left: it takes the corner, CPU makes room.
  const backup = await browser.middleOf(tile("Last backup"));
  await browser.drag(backup, [
    { x: backup.x - 60, y: backup.y + 4 },
    { x: backup.x - step.x * 8, y: backup.y },
  ]);
  check("a tile dragged onto another takes its place", (await box("Last backup")) === "0,0,2,2", await box("Last backup"));
  check("the tile that was there made room", (await box("CPU")) !== "0,0,3,2", await box("CPU"));

  // Pull the corner of "Services": one column wider, one row higher.
  const services = await browser.evaluate(`(() => {
    const grip = ${tile("Services")}.querySelector(".grip").getBoundingClientRect();
    return { x: grip.x + grip.width / 2, y: grip.y + grip.height / 2 };
  })()`);
  const servicesBefore = (await box("Services")).split(",").map(Number);
  await browser.drag(services, [
    { x: services.x + 10, y: services.y + 10 },
    { x: services.x + step.x, y: services.y + step.y },
  ]);
  const servicesAfter = (await box("Services")).split(",").map(Number);
  check(
    "pulling the corner resizes",
    servicesAfter[2] === servicesBefore[2] + 1 && servicesAfter[3] === servicesBefore[3] + 1,
    `${servicesBefore} -> ${servicesAfter}`,
  );

  // Escape while dragging puts the tile back.
  const atlas = await browser.middleOf(tile("Atlas"));
  const atlasBefore = await box("Atlas");
  await browser.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: atlas.x, y: atlas.y });
  await browser.send("Input.dispatchMouseEvent", { type: "mousePressed", x: atlas.x, y: atlas.y, button: "left", clickCount: 1 });
  await browser.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: atlas.x + step.x * 4, y: atlas.y + 20, button: "left", buttons: 1 });
  await browser.shot(join(folder, "dragging.png"), { full: true });
  await browser.press("Escape");
  await browser.send("Input.dispatchMouseEvent", { type: "mouseReleased", x: atlas.x + step.x * 4, y: atlas.y + 20, button: "left", clickCount: 1 });
  check("Escape during a drag puts the tile back", (await box("Atlas")) === atlasBefore, `${atlasBefore} -> ${await box("Atlas")}`);

  // ------------------------------------------------------------ keyboard
  await browser.evaluate(`${tile("Comet CPU")}.focus()`);
  const cometBefore = await box("Comet CPU");
  await browser.press("ArrowLeft");
  check("an arrow key moves the focused tile", (await box("Comet CPU")) !== cometBefore, `${cometBefore} -> ${await box("Comet CPU")}`);
  const cometMoved = (await box("Comet CPU")).split(",").map(Number);
  await browser.press("ArrowDown", { shift: true });
  const cometGrown = (await box("Comet CPU")).split(",").map(Number);
  check("Shift with an arrow key resizes it", cometGrown[3] === cometMoved[3] + 1, `${cometMoved} -> ${cometGrown}`);
  await browser.press("Delete");
  check("Delete removes it", (await box("Comet CPU")) === null);

  // -------------------------------------------------------------- remove
  await browser.clickOn(`${tile("Spent per day")}.querySelector('.tool[aria-label^="Remove"]')`);
  check("the cross removes a tile", (await box("Spent per day")) === null);

  // ----------------------------------------------------------------- add
  await browser.clickOn(buttonNamed("Add tile"));
  check("Add tile opens the picker", await browser.evaluate(`document.querySelector("dialog.picker")?.open === true`));
  await browser.clickOn(buttonNamed("Forecast", `document.querySelector("dialog.picker")`));
  check("a suggested tile is added with one click", (await box("Forecast")) !== null, await box("Forecast"));

  await browser.clickOn(buttonNamed("Add tile"));
  const tick = (label) =>
    `[...document.querySelectorAll("dialog.picker .source label.choice")].find((l) => l.querySelector("span").textContent === ${JSON.stringify(label)}).querySelector("input")`;
  await browser.clickOn(tick("Memory"));
  await browser.clickOn(tick("Disk free"));
  await browser.clickOn(tick("Uptime"));
  check(
    "three fields default to a table",
    (await browser.evaluate(`document.querySelector('dialog.picker input[name="view"]:checked')?.value`)) === "table",
  );
  check(
    "a sparkline is not offered for fields with no history",
    await browser.evaluate(`document.querySelector('dialog.picker input[name="view"][value="sparkline"]').disabled`),
  );
  await browser.clickOn(`document.querySelector('dialog.picker input[type="text"][maxlength="100"]')`);
  await browser.type("Atlas health");
  await new Promise((resolve) => setTimeout(resolve, 900));
  await browser.shot(join(folder, "picker.png"));
  const preview = await browser.evaluate(`document.querySelector("dialog.picker .preview .tile").innerText.replace(/\\s+/g, " ")`);
  check("the preview shows live values", /Atlas health .*Memory .*%.*Disk free .*GB.*Uptime/.test(preview), preview);
  await browser.clickOn(buttonNamed("Add tile", `document.querySelector("dialog.picker")`));
  check("a built tile is added", (await box("Atlas health")) !== null, await box("Atlas health"));

  // -------------------------------------------------------------- change
  await browser.clickOn(`${tile("This week")}.querySelector('.tool[aria-label^="Change"]')`);
  await browser.clickOn(`document.querySelector('dialog.picker input[name="view"][value="table"]')`);
  await browser.clickOn(buttonNamed("Save tile", `document.querySelector("dialog.picker")`));
  check("a tile's view can be changed", (await browser.evaluate(`${tile("This week")}.dataset.view`)) === "table");

  await browser.shot(join(folder, "edit-after.png"), { full: true });

  // ---------------------------------------------------------------- save
  await browser.clickOn(buttonNamed("Done"));
  await new Promise((resolve) => setTimeout(resolve, 600));
  const saved = await savedLayout();
  const titles = saved.tiles.map((t) => t.label);
  check("Done saves", saved.rev === 1, `rev ${saved.rev}`);
  check("Done locks the board again", (await browser.middleOf(buttonNamed("Edit"))) !== null && !(await browser.evaluate(`document.body.classList.contains("editing")`)));
  check("the saved layout has the new tiles and not the removed ones", titles.includes("Atlas health") && titles.includes("Forecast") && !titles.includes("Comet CPU") && !titles.includes("Spent per day"), titles.join(", "));
  check("no saved tiles overlap", !overlaps(saved.tiles));
  const built = saved.tiles.find((t) => t.label === "Atlas health");
  check(
    "the built tile is saved as chosen",
    built.view === "table" && built.fields.map((f) => `${f.app}.${f.source}.${f.field}`).join(" ") === "demo.server.memory demo.server.disk_free demo.server.uptime",
    JSON.stringify(built),
  );
  check("the changed tile is saved as a table", saved.tiles.find((t) => t.label === "This week").view === "table");
  check("the board shows the saved layout", (await box("Last backup")) === "0,0,2,2");

  // ------------------------------------------- saved on another screen meanwhile
  await browser.clickOn(buttonNamed("Edit"));
  const again = await browser.middleOf(tile("Last backup"));
  await browser.drag(again, [
    { x: again.x + 30, y: again.y + 3 },
    { x: again.x + step.x * 6, y: again.y },
  ]);
  const mine = await box("Last backup");
  const other = await savedLayout();
  other.tiles = other.tiles.slice(0, 3);
  const elsewhere = await fetch(new URL("/api/layout", url), { method: "PUT", headers: { "Content-Type": "application/json" }, body: JSON.stringify(other) });
  check("another screen saves first", elsewhere.ok && (await savedLayout()).rev === 2);
  await new Promise((resolve) => setTimeout(resolve, 6000)); // long enough for a data answer to bring the new rev
  check("the edits are not thrown away while editing", (await box("Last backup")) === mine && (await browser.evaluate(`document.querySelectorAll(".tile").length`)) > 3);
  await browser.clickOn(buttonNamed("Done"));
  await new Promise((resolve) => setTimeout(resolve, 400));
  check("Done then asks which layout to keep", await browser.evaluate(`document.querySelector("dialog.ask")?.open === true`));
  await browser.shot(join(folder, "conflict.png"));
  await browser.clickOn(buttonNamed("Keep my changes", `document.querySelector("dialog.ask")`));
  await new Promise((resolve) => setTimeout(resolve, 600));
  const final = await savedLayout();
  check("keeping mine saves it over the other one", final.rev === 3 && final.tiles.length > 3, `rev ${final.rev}, ${final.tiles.length} tiles`);

  // The refused save above is meant to happen; the browser notes it as a failed load.
  const problems = browser.problems.filter((problem) => !problem.includes("409 (Conflict)"));
  check("the page logged no errors", problems.length === 0, problems.join(" | "));
} catch (error) {
  failures += 1;
  console.log(`FAIL the run stopped: ${error.message}`);
  await browser.shot(join(folder, "stopped.png"), { full: true }).catch(() => {});
} finally {
  await browser.close();
}
console.log(failures === 0 ? "\nEdit mode works." : `\n${failures} thing(s) not as expected.`);
process.exit(failures === 0 ? 0 : 1);
