// The picker: choose what a tile shows. Pick an app, tick the fields you want
// (they are grouped by source), choose a view and a name. A live preview shows
// the tile as it will be. Nothing here is specific to any app: it is all built
// from what the apps say about themselves.

import { VIEWS, bestView, presetContent, sourceKey, tileModel, viewFits } from "./model.js";
import { drawTile, el } from "./views.js";

const TYPE_WORDS = { number: "number", text: "text", status: "status", series: "history", list: "list" };

function button(text, className = "") {
  const node = el("button", className, text);
  node.type = "button";
  return node;
}

/**
 * Opens the picker and resolves to what the tile should show, { label, view, fields },
 * or to null if the person backed out. `tile` is the tile to change, or null for a new one.
 * `context` is { apps, getData(sources), now() }.
 */
export function openPicker(context, tile) {
  const apps = [...context.apps.values()].filter((app) => app.catalogue);
  const adding = tile === null;

  // What is chosen so far.
  const picked = adding ? [] : tile.fields.map((field) => ({ ...field }));
  let appName = picked[0]?.app && context.apps.get(picked[0].app)?.catalogue ? picked[0].app : apps[0]?.name;
  let view = adding ? null : tile.view; // null: follow the best view for the fields
  let label = adding ? "" : tile.label;
  /** "app, source" → the param values set for that source. Only values that differ from the default are kept. */
  const params = new Map(picked.map((field) => [`${field.app}\n${field.source}`, { ...(field.params ?? {}) }]));
  const answers = new Map();
  const memory = {};

  const specOf = (ref) =>
    context.apps
      .get(ref.app)
      ?.catalogue?.sources.find((source) => source.id === ref.source)
      ?.fields.find((field) => field.id === ref.field);
  const types = () => picked.map((ref) => specOf(ref)?.type).filter(Boolean);
  const currentView = () => (view && viewFits(view, types()) ? view : bestView(types()));
  const paramsOf = (app, source) => params.get(`${app}\n${source}`) ?? {};
  const content = () => ({
    label: label.trim(),
    view: currentView(),
    fields: picked.map(({ app, source, field }) => {
      const set = paramsOf(app, source);
      return { app, source, ...(Object.keys(set).length > 0 ? { params: { ...set } } : {}), field };
    }),
  });

  return new Promise((resolve) => {
    const dialog = el("dialog", "picker");
    const title = el("h2", "", adding ? "Add a tile" : "Change this tile");
    const sources = el("div", "picker-sources");
    const result = el("div", "picker-result");
    const cancel = button("Cancel");
    const done = button(adding ? "Add tile" : "Save tile", "primary");
    const foot = el("footer", "picker-foot");
    foot.append(cancel, done);
    const columns = el("div", "picker-columns");
    columns.append(sources, result);
    dialog.append(title, columns, foot);

    let outcome = null;
    cancel.addEventListener("click", () => dialog.close());
    done.addEventListener("click", () => {
      outcome = content();
      dialog.close();
    });
    dialog.addEventListener("close", () => {
      dialog.remove();
      resolve(outcome);
    });

    // ------------------------------------------------------------ preview

    const previewTile = el("article", "tile");
    async function refreshPreview() {
      const tileNow = { id: "preview", ...content() };
      const draw = () => drawTile(previewTile, tileModel(tileNow, context.apps, answers, context.now()), memory);
      draw();
      // Ask for the sources the preview needs and does not have yet.
      const wanted = new Map(tileNow.fields.map((field) => [sourceKey(field), field]));
      const missing = [...wanted].filter(([key]) => !answers.has(key));
      if (missing.length === 0) return;
      try {
        const got = await context.getData(missing.map(([, { app, source, params: set }]) => ({ app, source, params: set ?? {} })));
        missing.forEach(([key], i) => answers.set(key, got[i]));
      } catch {
        return; // The preview stays as it is; the board will say if the server is gone.
      }
      if (dialog.open) drawTile(previewTile, tileModel({ id: "preview", ...content() }, context.apps, answers, context.now()), memory);
    }

    // ------------------------------------------------------- right: the tile

    function drawResult() {
      const chosen = el("ol", "picked");
      picked.forEach((ref, i) => {
        const spec = specOf(ref);
        const app = context.apps.get(ref.app)?.catalogue;
        const source = app?.sources.find((candidate) => candidate.id === ref.source);
        const item = el("li");
        const words = el("span", "picked-name");
        words.append(el("strong", "", spec?.label ?? ref.field), el("span", "", `${app?.title ?? ref.app}, ${source?.title ?? ref.source}`));
        item.append(words);
        if (i > 0) {
          const up = button("Up", "small");
          up.title = "Show this one earlier on the tile";
          up.addEventListener("click", () => {
            [picked[i - 1], picked[i]] = [picked[i], picked[i - 1]];
            drawResult();
          });
          item.append(up);
        }
        const out = button("Remove", "small");
        out.addEventListener("click", () => {
          picked.splice(i, 1);
          drawSources();
          drawResult();
        });
        item.append(out);
        chosen.append(item);
      });

      const views = el("fieldset", "views");
      views.append(el("legend", "", "Shown as"));
      const fits = types();
      for (const [id, name] of VIEWS) {
        const choice = el("label", "choice");
        const radio = el("input");
        radio.type = "radio";
        radio.name = "view";
        radio.value = id;
        radio.checked = picked.length > 0 && currentView() === id;
        radio.disabled = !viewFits(id, fits);
        if (radio.disabled && picked.length > 0) choice.title = "The chosen fields cannot be shown this way";
        radio.addEventListener("change", () => {
          view = id;
          drawResult();
        });
        choice.append(radio, el("span", "", name));
        views.append(choice);
      }

      const name = el("label", "field");
      const input = el("input");
      input.type = "text";
      input.maxLength = 100;
      input.value = label;
      input.placeholder = specOf(picked[0] ?? {})?.label ?? "";
      input.addEventListener("input", () => {
        label = input.value;
        refreshPreview();
      });
      name.append(el("span", "", "Name on the tile (optional)"), input);

      const preview = el("div", "preview");
      preview.dataset.view = currentView();
      preview.append(previewTile);

      const empty = el("p", "hint", "Tick one or more fields on the left. The first one is the main value.");
      result.replaceChildren(
        el("h3", "", "The tile"),
        ...(picked.length === 0 ? [empty] : [chosen, views, name, el("h3", "", "Preview"), preview]),
      );
      done.disabled = picked.length === 0;
      if (picked.length > 0) refreshPreview();
    }

    // ------------------------------------------------- left: what there is

    function paramControl(app, source, param) {
      const set = paramsOf(app.name, source.id);
      const wrap = el("label", "field");
      wrap.append(el("span", "", param.label ?? param.name));
      let control;
      if (param.type === "choice") {
        control = el("select");
        for (const option of param.options) {
          const node = el("option", "", option.label ?? option.value);
          node.value = option.value;
          control.append(node);
        }
      } else {
        control = el("input");
        control.type = param.type === "number" ? "number" : "text";
        if (param.type === "number") control.step = "any";
      }
      control.value = set[param.name] ?? param.default;
      control.addEventListener("change", () => {
        const next = { ...paramsOf(app.name, source.id) };
        if (control.value === param.default || control.value === "") delete next[param.name];
        else next[param.name] = control.value;
        params.set(`${app.name}\n${source.id}`, next);
        drawResult();
      });
      wrap.append(control);
      return wrap;
    }

    function drawSources() {
      const app = context.apps.get(appName);
      const parts = [];

      if (apps.length > 1) {
        const choose = el("label", "field");
        const select = el("select");
        for (const candidate of apps) {
          const option = el("option", "", candidate.catalogue.title);
          option.value = candidate.name;
          select.append(option);
        }
        select.value = appName;
        select.addEventListener("change", () => {
          appName = select.value;
          drawSources();
        });
        choose.append(el("span", "", "App"), select);
        parts.push(choose);
      }

      // The app's own suggestions: one click makes the tile.
      const presets = app.catalogue.presets ?? [];
      if (adding && presets.length > 0) {
        const box = el("div", "presets");
        box.append(el("h3", "", `Suggested by ${app.catalogue.title}`));
        const row = el("div", "preset-row");
        for (const preset of presets) {
          const quick = button(preset.title, "small");
          quick.addEventListener("click", () => {
            const made = presetContent(app.name, preset);
            const kinds = made.fields.map((ref) => specOf(ref)?.type).filter(Boolean);
            made.view = made.view && viewFits(made.view, kinds) ? made.view : bestView(kinds);
            outcome = made;
            dialog.close();
          });
          row.append(quick);
        }
        box.append(row, el("h3", "", "Or build your own"));
        parts.push(box);
      }

      for (const source of app.catalogue.sources) {
        const group = el("fieldset", "source");
        group.append(el("legend", "", source.title));
        for (const param of source.params ?? []) group.append(paramControl(app, source, param));
        for (const field of source.fields) {
          const check = el("label", "choice");
          const box = el("input");
          box.type = "checkbox";
          const at = () => picked.findIndex((ref) => ref.app === app.name && ref.source === source.id && ref.field === field.id);
          box.checked = at() >= 0;
          box.addEventListener("change", () => {
            if (box.checked) picked.push({ app: app.name, source: source.id, field: field.id });
            else picked.splice(at(), 1);
            drawResult();
          });
          const hint = field.unit ? `${TYPE_WORDS[field.type]}, ${field.unit}` : TYPE_WORDS[field.type];
          check.append(box, el("span", "", field.label), el("small", "", hint));
          group.append(check);
        }
        parts.push(group);
      }
      sources.replaceChildren(...parts);
    }

    if (apps.length === 0) {
      columns.replaceChildren(el("p", "hint", "No app is reachable right now, so there is nothing to choose from."));
      done.disabled = true;
    } else {
      drawSources();
      drawResult();
    }
    document.body.append(dialog);
    dialog.showModal();
  });
}
