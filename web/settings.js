// Settings: the apps this dashboard shows, and how to connect another one.
// "Connect an app" hands over one address that holds everything a developer or
// an agent needs, checks an app, and adds it.

import { el, statusIcon } from "./views.js";

function button(text, className = "") {
  const node = el("button", className, text);
  node.type = "button";
  return node;
}

async function call(method, path, body) {
  const response = await fetch(path, {
    method,
    headers: body ? { "Content-Type": "application/json" } : {},
    body: body ? JSON.stringify(body) : undefined,
  });
  return { status: response.status, ok: response.ok, body: await response.json().catch(() => ({})) };
}

/** What the checker found, as a list a person can act on. */
function reportList(report) {
  const list = el("ul", "report");
  const line = (status, text) => {
    const item = el("li");
    item.append(statusIcon(status), el("span", "", text));
    list.append(item);
  };
  if (report.summary) line(report.ok ? "ok" : "warn", report.summary);
  for (const problem of report.problems ?? []) line("crit", problem);
  for (const source of report.sources ?? []) {
    if (source.ok) line("ok", `${source.title}: follows the contract`);
    for (const problem of source.problems) line("crit", `${source.title}: ${problem}`);
  }
  return list;
}

/**
 * Opens the settings. `context` is { getApps(), refreshApps() }: the apps as last
 * loaded ({ manageApps, apps: Map }), and a way to load them again.
 */
export function openSettings(context) {
  const dialog = el("dialog", "settings");
  const appsBox = el("div");
  const result = el("div", "settings-result");
  result.setAttribute("role", "status");
  const kitAddress = `${location.origin}/connect.md`;

  function drawApps() {
    const { apps } = context.getApps();
    const list = el("ul", "app-list");
    for (const app of apps.values()) {
      const item = el("li");
      item.append(statusIcon(app.ok ? "ok" : "crit"));
      const words = el("span", "app-words");
      words.append(el("strong", "", app.catalogue ? `${app.catalogue.title} (${app.name})` : app.name));
      words.append(el("span", "", app.ok ? app.base_url : `${app.base_url}: ${app.error}`));
      item.append(words);
      if (app.managed) {
        const out = button("Remove", "small");
        out.addEventListener("click", async () => {
          // Asked twice: tiles that show this app stop working.
          if (out.textContent === "Remove") {
            out.textContent = "Really remove?";
            return;
          }
          const answer = await call("DELETE", `/api/apps/${encodeURIComponent(app.name)}`);
          result.replaceChildren(el("p", "", answer.ok ? `${app.name} removed.` : `Not removed: ${answer.body.error}.`));
          await context.refreshApps();
          drawApps();
        });
        item.append(out);
      }
      list.append(item);
    }
    if (apps.size === 0) list.append(el("li", "", "No apps yet."));
    appsBox.replaceChildren(list);
  }

  // ------------------------------------------------- for the app's developer

  const brief = el("textarea", "brief");
  brief.readOnly = true;
  brief.rows = 3;
  brief.value = `Connect this app to my tileboard dashboard. Everything you need is at ${kitAddress} (read it first and follow it: what to build, the full contract, how to check your work and how to register the app).`;
  const copy = button("Copy", "small");
  copy.addEventListener("click", async () => {
    brief.select();
    try {
      // The clipboard needs https or localhost; on a plain http address the old way still works.
      if (navigator.clipboard) await navigator.clipboard.writeText(brief.value);
      else document.execCommand("copy");
      copy.textContent = "Copied";
    } catch {
      copy.textContent = "Select and copy by hand";
    }
  });
  const kitLink = el("a", "", "Open the connect page");
  kitLink.href = "/connect.md";
  kitLink.target = "_blank";
  kitLink.rel = "noopener";
  const handOver = el("div", "hand-over");
  handOver.append(copy, kitLink);

  // -------------------------------------------------------------- add it here

  const form = el("div", "add-app");
  function drawForm() {
    if (!context.getApps().manageApps) {
      form.replaceChildren(
        el("p", "hint", "Adding apps from this page is turned off. To turn it on, put this line near the top of config.toml and restart the dashboard:"),
        el("pre", "", "manage_apps = true"),
        el("p", "hint", "Or write the app into config.toml by hand; the connect page shows the block."),
      );
      return;
    }
    const input = (label, placeholder, type = "text") => {
      const wrap = el("label", "field");
      const node = el("input");
      node.type = type;
      node.placeholder = placeholder;
      node.autocomplete = "off";
      wrap.append(el("span", "", label), node);
      return [wrap, node];
    };
    const [nameField, name] = input("Name (short, lowercase; tiles will refer to it)", "my-app");
    const [addressField, address] = input("Address the dashboard server can call", "http://127.0.0.1:8080");
    const [tokenField, token] = input("Token (only if the app asks for one)", "", "password");
    const check = button("Check");
    const add = button("Add app", "primary");
    const row = el("div", "ask-row");
    row.append(check, add);
    form.replaceChildren(nameField, addressField, tokenField, row);

    const entry = () => ({
      base_url: address.value.trim(),
      ...(token.value ? { token: token.value } : {}),
    });
    const show = (answer) => {
      const parts = [];
      if (answer.body.error) parts.push(el("p", "", `${answer.body.error}.`));
      for (const problem of answer.body.problems ?? []) parts.push(el("p", "", problem));
      const report = answer.body.report ?? (answer.body.sources ? answer.body : null);
      if (report) parts.push(reportList(report));
      result.replaceChildren(...parts);
    };
    check.addEventListener("click", async () => {
      result.replaceChildren(el("p", "", "Checking…"));
      show(await call("POST", "/api/check", entry()));
    });
    add.addEventListener("click", async () => {
      result.replaceChildren(el("p", "", "Checking, then adding…"));
      const answer = await call("POST", "/api/apps", { name: name.value.trim(), ...entry() });
      if (!answer.ok) {
        show(answer);
        return;
      }
      result.replaceChildren(el("p", "", `Added: ${answer.body.summary ?? answer.body.name}. Its fields are now in Edit, Add tile.`));
      name.value = "";
      address.value = "";
      token.value = "";
      await context.refreshApps();
      drawApps();
    });
  }

  const close = button("Close");
  close.addEventListener("click", () => dialog.close());
  const foot = el("footer", "picker-foot");
  foot.append(close);
  const body = el("div", "settings-body");
  body.append(
    el("h3", "", "Apps"),
    appsBox,
    el("h3", "", "Connect an app"),
    el("p", "hint", "Give this to whoever works on the app: a developer, or an AI agent in the app's code. The address holds everything they need."),
    brief,
    handOver,
    el("h3", "", "Add it here"),
    form,
    result,
  );
  dialog.append(el("h2", "", "Settings"), body, foot);
  dialog.addEventListener("close", () => dialog.remove());

  drawApps();
  drawForm();
  document.body.append(dialog);
  dialog.showModal();
}
