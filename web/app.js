// Placeholder page until view mode is built (step 3 of docs/plan.md):
// it shows which apps the dashboard can reach and what they offer.
// Everything that comes from an app is put on the page as text, never as HTML.

function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

async function getJson(path) {
  const response = await fetch(path);
  if (!response.ok) throw new Error(`${path} answered ${response.status}`);
  return response.json();
}

function appCard(app) {
  const card = element("section", "app");
  const title = app.catalogue ? `${app.catalogue.title} (${app.name})` : app.name;
  card.append(element("h2", "", title));
  card.append(element("span", app.ok ? "state ok" : "state down", app.ok ? "reachable" : "not reachable"));
  if (app.error) card.append(element("p", "muted", app.error));

  if (app.catalogue) {
    const list = element("ul");
    for (const source of app.catalogue.sources) {
      const fields = source.fields.map((field) => field.label).join(", ");
      list.append(element("li", "", `${source.title}: ${fields}`));
    }
    card.append(list);
  }
  return card;
}

async function show() {
  const summary = document.getElementById("summary");
  const container = document.getElementById("apps");
  try {
    const [{ apps }, layout] = await Promise.all([getJson("/api/apps"), getJson("/api/layout")]);
    const reachable = apps.filter((app) => app.ok).length;
    summary.textContent = `${reachable} of ${apps.length} apps reachable, ${layout.tiles.length} tiles in the layout.`;
    container.replaceChildren(...apps.map(appCard));
  } catch (error) {
    summary.textContent = `The dashboard server did not answer: ${error.message}`;
  }
}

show();
