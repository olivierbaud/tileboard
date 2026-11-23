// Drives a headless Edge or Chrome through its DevTools protocol, so the page can be
// looked at and poked for real: screenshots, pointer and key events, console errors.
// Uses only what ships with Node 22+ (no packages) and a browser already on the machine.
//
// A development tool. tileboard itself never needs it.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const CANDIDATES = [
  process.env.TILEBOARD_BROWSER,
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
  "/usr/bin/google-chrome",
  "/usr/bin/microsoft-edge",
];

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Starts a headless browser with one page and returns the means to drive it.
 * Set TILEBOARD_BROWSER to the browser's path if it is not found.
 */
export async function openBrowser({ port = 9333 } = {}) {
  const path = CANDIDATES.find((candidate) => candidate && existsSync(candidate));
  if (!path) throw new Error("No Edge or Chrome found. Set TILEBOARD_BROWSER to its path.");

  const profile = mkdtempSync(join(tmpdir(), "tileboard-browser-"));
  const child = spawn(
    path,
    ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`, "--no-first-run", "--hide-scrollbars", "about:blank"],
    { stdio: "ignore" },
  );

  // Wait for the browser to answer, then find its page.
  let target;
  for (let tries = 0; !target; tries += 1) {
    if (tries > 100) throw new Error("The browser did not start.");
    await sleep(100);
    try {
      const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      target = pages.find((page) => page.type === "page");
    } catch {
      // Not up yet.
    }
  }

  const socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  let nextId = 0;
  const waiting = new Map();
  /** Errors and warnings the page logged: script exceptions, failed loads, policy violations. */
  const problems = [];
  socket.addEventListener("message", (message) => {
    const data = JSON.parse(message.data);
    if (data.id !== undefined) {
      const { resolve, reject } = waiting.get(data.id);
      waiting.delete(data.id);
      if (data.error) reject(new Error(`${data.error.message}`));
      else resolve(data.result);
    } else if (data.method === "Runtime.exceptionThrown") {
      const details = data.params.exceptionDetails;
      problems.push(`exception: ${details.exception?.description ?? details.text}`);
    } else if (data.method === "Log.entryAdded" && ["error", "warning"].includes(data.params.entry.level)) {
      problems.push(`${data.params.entry.level}: ${data.params.entry.text}`);
    } else if (data.method === "Runtime.consoleAPICalled" && ["error", "warning"].includes(data.params.type)) {
      problems.push(`console.${data.params.type}: ${data.params.args.map((arg) => arg.value ?? arg.description).join(" ")}`);
    }
  });

  /** One DevTools protocol command, e.g. send("Page.navigate", { url }). */
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      nextId += 1;
      waiting.set(nextId, { resolve, reject });
      socket.send(JSON.stringify({ id: nextId, method, params }));
    });

  await send("Page.enable");
  await send("Runtime.enable");
  await send("Log.enable");

  return {
    send,
    problems,

    /** The page's size, and whether it should say it prefers a light or a dark scheme. */
    async look({ width, height, scheme = "light" }) {
      await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: width < 600 });
      await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: scheme }] });
    },

    /** Opens `url` and gives the page `settle` ms to load its data and draw. */
    async goto(url, settle = 1500) {
      await send("Page.navigate", { url });
      await sleep(settle);
    },

    /** Runs an expression in the page and returns its value. */
    async evaluate(expression) {
      const { result, exceptionDetails } = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
      if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
      return result.value;
    },

    /** Moves the pointer to a place on the page. */
    async moveTo(x, y) {
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
      await sleep(60);
    },

    /** Presses the left button, moves through `points`, and lets go at the last one. */
    async drag(from, points) {
      await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: from.x, y: from.y });
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x: from.x, y: from.y, button: "left", clickCount: 1 });
      for (const point of points) {
        await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: point.x, y: point.y, button: "left", buttons: 1 });
        await sleep(30);
      }
      const last = points.at(-1) ?? from;
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: last.x, y: last.y, button: "left", clickCount: 1 });
      await sleep(60);
    },

    async click(x, y) {
      await send("Input.dispatchMouseEvent", { type: "mousePressed", x, y, button: "left", clickCount: 1 });
      await send("Input.dispatchMouseEvent", { type: "mouseReleased", x, y, button: "left", clickCount: 1 });
      await sleep(60);
    },

    /** Presses a key by its name: "ArrowLeft", "Tab", "Escape", "Enter", "Delete". `shift` holds Shift down. */
    async press(key, { shift = false } = {}) {
      const codes = { ArrowLeft: 37, ArrowUp: 38, ArrowRight: 39, ArrowDown: 40, Tab: 9, Escape: 27, Enter: 13, Delete: 46 };
      const params = { key, code: key, windowsVirtualKeyCode: codes[key], modifiers: shift ? 8 : 0 };
      await send("Input.dispatchKeyEvent", { type: "keyDown", ...params });
      await send("Input.dispatchKeyEvent", { type: "keyUp", ...params });
      await sleep(60);
    },

    /** Types text into whatever has the focus. */
    async type(text) {
      await send("Input.insertText", { text });
      await sleep(60);
    },

    /** The middle of the first element `find` (an expression run in the page) gives, as { x, y }; null if none. */
    async middleOf(find) {
      return this.evaluate(`(() => {
        const node = ${find};
        if (!node) return null;
        node.scrollIntoView({ block: "nearest" });
        const box = node.getBoundingClientRect();
        return { x: box.x + box.width / 2, y: box.y + box.height / 2, width: box.width, height: box.height };
      })()`);
    },

    /** Clicks the element `find` gives. Throws if there is none. */
    async clickOn(find) {
      const at = await this.middleOf(find);
      if (!at) throw new Error(`nothing to click: ${find}`);
      await this.moveTo(at.x, at.y);
      await this.click(at.x, at.y);
      await sleep(120);
    },

    /**
     * Saves a picture of the page: the whole page and not just the window with `full`,
     * or one part of it, enlarged, with `clip` ({ x, y, width, height, scale }).
     */
    async shot(file, { full = false, clip } = {}) {
      const params = { format: "png", captureBeyondViewport: full || clip !== undefined };
      if (clip) params.clip = { scale: 1, ...clip };
      const { data } = await send("Page.captureScreenshot", params);
      writeFileSync(file, Buffer.from(data, "base64"));
      return file;
    },

    async close() {
      try {
        await send("Browser.close");
      } catch {
        child.kill();
      }
      await sleep(300);
      rmSync(profile, { recursive: true, force: true, maxRetries: 5 });
    },
  };
}
