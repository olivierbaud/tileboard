// Takes the pictures the README shows, from the demo (made-up data only).
//
//   node tools/pictures.mjs [url] [folder]        (folder defaults to docs/images)
//
// Needs a freshly started `tileboard --demo`.

import { mkdirSync } from "node:fs";
import { join } from "node:path";

import { openBrowser } from "./browser.mjs";

const url = process.argv[2] ?? "http://127.0.0.1:4700/";
const folder = process.argv[3] ?? "docs/images";
mkdirSync(folder, { recursive: true });

const browser = await openBrowser({ port: 9338 });
const named = (text, within = "document") =>
  `[...${within}.querySelectorAll("button")].find((b) => b.textContent.trim() === ${JSON.stringify(text)})`;
const tick = (label) =>
  `[...document.querySelectorAll("dialog.picker .source label.choice")].find((l) => l.querySelector("span").textContent === ${JSON.stringify(label)}).querySelector("input")`;

try {
  await browser.look({ width: 1280, height: 716, scheme: "light" });
  await browser.goto(url, 2500);
  console.log(await browser.shot(join(folder, "board-light.png")));

  await browser.look({ width: 1280, height: 716, scheme: "dark" });
  await browser.goto(url, 2500);
  console.log(await browser.shot(join(folder, "board-dark.png")));

  // Edit mode with the picker open on a tile being built.
  await browser.look({ width: 1280, height: 760, scheme: "light" });
  await browser.goto(url, 2000);
  await browser.clickOn(named("Edit"));
  await browser.clickOn(named("Add tile"));
  await browser.clickOn(tick("Memory"));
  await browser.clickOn(tick("Disk free"));
  await browser.clickOn(tick("Uptime"));
  await browser.clickOn(`document.querySelector('dialog.picker input[type="text"][maxlength="100"]')`);
  await browser.type("Atlas health");
  await new Promise((resolve) => setTimeout(resolve, 1000));
  console.log(await browser.shot(join(folder, "picker.png")));
  await browser.clickOn(named("Cancel", `document.querySelector("dialog.picker")`));
  console.log(await browser.shot(join(folder, "edit.png")));

  await browser.look({ width: 390, height: 844, scheme: "light" });
  await browser.goto(url, 2500);
  console.log(await browser.shot(join(folder, "phone.png"), { full: true }));

  if (browser.problems.length > 0) console.log(`PAGE ERRORS:\n  ${browser.problems.join("\n  ")}`);
} finally {
  await browser.close();
}
