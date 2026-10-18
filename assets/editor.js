// The SVG division editor: the plan view, compartment selection, split,
// rename, merge, and divider drags.
//
// Every edit is a design mutation that app.js sends to `/api/plan`; the
// editor never decides validity. The one local arithmetic is the *drag and
// split clamp*: it derives the legal integer range from `/api/standard`
// (minimum compartment and divider thickness) and the parent rectangle the
// plan response carries, so a pointer can never leave the SYS-1.0 model
// (ADR-0002; the server still validates every committed edit).

import * as panels from "./panels.js";

let ctx = null;

/** Wires the editor to the app store and actions. */
export function setEditorContext(context) {
  ctx = context;
}

const SVG_NS = "http://www.w3.org/2000/svg";

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function mm(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) return "—";
  const rounded = Math.round(value * 10) / 10;
  return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

/** The editor view: toolbar, plan, selection toolbar, and the sidebar. */
export function renderEditorView(root) {
  const { state } = ctx;

  if (!state.standard) {
    if (!state.standardError) ctx.actions.ensureStandard();
    root.innerHTML = state.standardError
      ? `<div class="errors"><p>${escapeHtml(state.standardError.message)}</p></div>`
      : `<p class="hint">loading the system standard…</p>`;
    return;
  }

  const design = state.design;
  if (!design) {
    root.innerHTML = `
      <section class="editor-view">
        <div class="editor-toolbar"><div class="badges">${panels.badgesHtml()}</div></div>
        <div class="errors">
          <p>This spec cannot be edited: it does not parse.</p>
          ${errorList(state.openErrors)}
        </div>
        <p><button type="button" class="btn primary" data-action="start-preset">Start from the calibrated preset</button></p>
        ${rawSpecHtml()}
      </section>`;
    bindActions(root);
    return;
  }

  const plan = state.plan;
  const planError = state.planError;
  const blocked = !plan || !plan.valid || Boolean(planError);

  root.innerHTML = `
  <section class="editor-view">
    <div class="editor-toolbar">
      <div class="badges">${panels.badgesHtml()}</div>
      <span class="spacer"></span>
      <div class="toolbar-actions">
        <button type="button" class="btn" data-action="undo" ${state.undo.length ? "" : "disabled"}>Undo</button>
        <button type="button" class="btn" data-action="redo" ${state.redo.length ? "" : "disabled"}>Redo</button>
        <button type="button" class="btn" data-action="save">Save</button>
        <button type="button" class="btn" data-action="save-as">Save as</button>
        <button type="button" class="btn" data-action="reset">Reset to preset</button>
        <button type="button" class="btn" data-action="build" ${blocked ? "disabled" : ""}>Build</button>
        <button type="button" class="btn" data-action="export" ${blocked ? "disabled" : ""}>Export</button>
        <button type="button" class="btn" data-action="publish" ${blocked ? "disabled" : ""}>Publish</button>
        <button type="button" class="btn primary" data-action="preview" ${blocked ? "disabled" : ""}>Preview in 3D</button>
      </div>
    </div>

    ${planError ? `<div class="errors"><p>${escapeHtml(planError.message)}</p></div>` : ""}
    ${state.openErrors.length ? `<div class="errors"><p>The spec on disk has errors; fixing them here and saving replaces the file.</p>${errorList(state.openErrors)}</div>` : ""}
    ${plan && !plan.valid ? `<div class="errors"><p>The spec violates the SYS-1.0 rules; saving and building are blocked until it is valid.</p>${errorList(plan.errors)}</div>` : ""}
    ${plan ? warningsHtml(plan.warnings) : ""}

    <div class="editor-body">
      <div class="plan-panel">
        ${planSvgHtml()}
        ${selectionToolbarHtml()}
      </div>
      <aside class="sidebar">
        ${panels.designFormHtml()}
        ${panels.jobPanelHtml()}
        ${panels.exportPanelHtml()}
        ${panels.publishPanelHtml()}
        ${panels.indexPanelHtml()}
        ${panels.fitAssistantHtml()}
        ${panels.recipeHtml()}
        <section class="panel">
          <h2>Registry</h2>
          <div class="toolbar-actions">
            <button type="button" class="btn" data-action="index">Regenerate index</button>
            <button type="button" class="btn danger" data-action="discard-draft">Discard draft</button>
          </div>
          <p class="hint">Regenerating writes <code>boxes/INDEX.md</code> from every spec; a spec with errors blocks the whole index.</p>
        </section>
      </aside>
    </div>
  </section>`;

  bindActions(root);
  panels.bindDesignForm(root);
  panels.bindFitAssistant(root);
  bindPlan(root);
  bindSelectionToolbar(root);
}

/** Every `data-action` element runs one app action. */
function bindActions(root) {
  root.querySelectorAll("[data-action]").forEach((element) => {
    const handler = ctx.actions[element.dataset.action];
    if (!handler) return;
    element.addEventListener("click", (event) => {
      event.preventDefault();
      handler(element);
    });
  });
}

function errorList(errors) {
  if (!errors || errors.length === 0) return "";
  return `<ul>${errors.map((error) => `<li>${escapeHtml(error)}</li>`).join("")}</ul>`;
}

/** Non-contents warnings (contents warnings live in the form). */
function warningsHtml(warnings) {
  const shown = (warnings ?? []).filter((warning) => !warning.startsWith("contents."));
  if (shown.length === 0) return "";
  return `<div class="warnings"><p>Warnings (the spec is still valid)</p>${errorList(shown)}</div>`;
}

function rawSpecHtml() {
  const { state } = ctx;
  if (!state.savedText) return "";
  return `<details class="panel" data-key="raw"><summary>Raw spec.yaml (not editable here)</summary><pre class="diagnostic">${escapeHtml(state.savedText)}</pre></details>`;
}

// ----------------------------------------------------------------- plan

function planSvgHtml() {
  const { state } = ctx;
  const plan = state.plan;
  if (!plan) {
    return `<div class="empty">Resolving the plan…</div>`;
  }
  if (plan.leaves.length === 0) {
    return `<div class="empty">The footprint or height is outside the system limits; fix the spec form to see the plan.</div>`;
  }

  const body = plan.body;
  const standard = state.standard;
  const wall = (body.outer_width_mm - body.interior_width_mm) / 2;
  const unit = Math.max(body.outer_width_mm, body.outer_depth_mm) / 40;
  const pad = 6;
  const selection = state.selection;
  const parts = [];

  parts.push(
    `<rect class="wall" x="0" y="0" width="${body.outer_width_mm}" height="${body.outer_depth_mm}" rx="${standard.walls.outer_radius_mm}"/>`,
  );
  parts.push(
    `<rect class="interior" x="${wall}" y="${wall}" width="${body.interior_width_mm}" height="${body.interior_depth_mm}"/>`,
  );

  for (const leaf of plan.leaves) {
    const selected = selection?.kind === "leaf" && selection.path === leaf.path;
    parts.push(
      `<rect class="leaf${selected ? " selected" : ""}" data-leaf="${escapeHtml(leaf.path)}" x="${wall + leaf.x_mm}" y="${wall + leaf.y_mm}" width="${leaf.width_mm}" height="${leaf.depth_mm}"/>`,
    );
  }
  for (const leaf of plan.leaves) {
    const cx = wall + leaf.x_mm + leaf.width_mm / 2;
    const cy = wall + leaf.y_mm + leaf.depth_mm / 2;
    if (leaf.width_mm >= 22 && leaf.depth_mm >= 10) {
      parts.push(
        `<text class="leaf-name" x="${cx}" y="${cy - unit * 0.8}" font-size="${unit * 1.5}">${escapeHtml(leaf.compartment ?? "—")}</text>`,
      );
      if (leaf.width_mm >= 30 && leaf.depth_mm >= 16) {
        parts.push(
          `<text class="leaf-size" x="${cx}" y="${cy + unit * 0.9}" font-size="${unit * 1.05}">${mm(leaf.width_mm)} × ${mm(leaf.depth_mm)}</text>`,
        );
      }
    }
  }
  for (const divider of plan.dividers) {
    const selected = selection?.kind === "divider" && selection.path === divider.path;
    parts.push(
      `<rect class="divider${selected ? " selected" : ""}" data-divider="${escapeHtml(divider.path)}" x="${wall + divider.x_mm}" y="${wall + divider.y_mm}" width="${divider.width_mm}" height="${divider.depth_mm}"/>`,
    );
  }

  return `<svg id="plan" class="plan-svg" viewBox="${-pad} ${-pad} ${body.outer_width_mm + 2 * pad} ${body.outer_depth_mm + 2 * pad}" role="img" aria-label="division tree plan">${parts.join("")}</svg>`;
}

/** The selection toolbar below the plan. */
function selectionToolbarHtml() {
  const { state } = ctx;
  const plan = state.plan;
  const selection = state.selection;
  if (!plan) return "";

  if (!selection) {
    return `<div class="selection-toolbar"><span class="hint">Click a compartment to split, rename, or merge it; drag a divider (or click it) to move it to an exact position.</span></div>`;
  }

  if (selection.kind === "leaf") {
    const leaf = plan.leaves.find((candidate) => candidate.path === selection.path);
    if (!leaf) return "";
    const standard = state.standard;
    const minimum = 2 * standard.dividers.min_leaf_mm + standard.dividers.thickness_mm;
    const canSplitX = leaf.width_mm >= minimum;
    const canSplitY = leaf.depth_mm >= minimum;
    return `
    <div class="selection-toolbar">
      <span class="title">Compartment ${leaf.compartment ? `“${escapeHtml(leaf.compartment)}”` : "(unnamed)"}</span>
      <span class="range">${mm(leaf.width_mm)} × ${mm(leaf.depth_mm)} mm</span>
      <button type="button" class="btn small" data-editor="split-x" ${canSplitX ? "" : "disabled"} title="Split at the clicked position (x axis)">Split ↔ x</button>
      <button type="button" class="btn small" data-editor="split-y" ${canSplitY ? "" : "disabled"} title="Split at the clicked position (y axis)">Split ↕ y</button>
      <button type="button" class="btn small" data-editor="rename">Rename</button>
      <button type="button" class="btn small danger" data-editor="merge">Merge away</button>
      ${canSplitX && canSplitY ? "" : `<span class="range">A compartment needs ${minimum} mm to split.</span>`}
    </div>`;
  }

  const divider = plan.dividers.find((candidate) => candidate.path === selection.path);
  if (!divider) return "";
  const standard = state.standard;
  const minimum = standard.dividers.min_leaf_mm;
  const size = divider.axis === "x" ? divider.parent.width_mm : divider.parent.depth_mm;
  const maximum = Math.max(minimum, Math.round(size - minimum - standard.dividers.thickness_mm));
  const at = Math.round(
    divider.axis === "x" ? divider.x_mm - divider.parent.x_mm : divider.y_mm - divider.parent.y_mm,
  );
  return `
  <div class="selection-toolbar">
    <span class="title">Divider ${divider.axis === "x" ? "x" : "y"}</span>
    <label class="check">
      <span>Position (mm)</span>
      <input type="number" id="divider-at" min="${minimum}" max="${maximum}" step="1" value="${at}" data-divider-path="${escapeHtml(divider.path)}">
    </label>
    <button type="button" class="btn small" data-editor="apply-divider">Apply</button>
    <span class="range">legal range ${minimum}–${maximum} mm from the compartment edge</span>
  </div>`;
}

/** Binds the plan: selection clicks and divider drags. */
function bindPlan(root) {
  const svg = root.querySelector("#plan");
  if (!svg) return;

  svg.addEventListener("click", (event) => {
    const leaf = event.target.closest("[data-leaf]");
    if (leaf) {
      const point = svgPoint(svg, event);
      const wall = interiorOffset();
      ctx.actions.selectLeaf(leaf.dataset.leaf, { x: point.x - wall, y: point.y - wall });
      return;
    }
    const divider = event.target.closest("[data-divider]");
    if (divider) ctx.actions.selectDivider(divider.dataset.divider);
  });

  svg.addEventListener("pointerdown", (event) => {
    const divider = event.target.closest("[data-divider]");
    if (!divider) return;
    startDrag(svg, divider, event);
  });
}

/** The editor's own buttons (split, rename, merge, exact position). */
function bindSelectionToolbar(root) {
  const on = (name, handler) => {
    const element = root.querySelector(`[data-editor="${name}"]`);
    if (element) element.addEventListener("click", handler);
  };
  on("split-x", () => splitSelected("x"));
  on("split-y", () => splitSelected("y"));
  on("rename", renameSelected);
  on("merge", mergeSelected);

  const input = root.querySelector("#divider-at");
  const apply = root.querySelector('[data-editor="apply-divider"]');
  if (input && apply) {
    const commit = () => {
      const minimum = Number(input.min);
      const maximum = Number(input.max);
      const value = clamp(Math.round(Number(input.value)), minimum, maximum);
      ctx.actions.setDividerAt(input.dataset.dividerPath, value);
    };
    apply.addEventListener("click", commit);
    input.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        commit();
      }
    });
  }
}

function selectedLeaf() {
  const { state } = ctx;
  if (state.selection?.kind !== "leaf") return null;
  return state.plan?.leaves.find((leaf) => leaf.path === state.selection.path) ?? null;
}

/** Splits the selected compartment at the clicked position, clamped. */
function splitSelected(axis) {
  const { state } = ctx;
  const selection = state.selection;
  const leaf = selectedLeaf();
  if (!leaf || !selection) return;
  const standard = state.standard;
  const minimum = standard.dividers.min_leaf_mm;
  const horizontal = axis === "x";
  const size = horizontal ? leaf.width_mm : leaf.depth_mm;
  const maximum = Math.max(minimum, Math.round(size - minimum - standard.dividers.thickness_mm));
  const clicked = horizontal ? selection.at.x - leaf.x_mm : selection.at.y - leaf.y_mm;
  ctx.actions.splitLeaf(leaf.path, axis, clamp(Math.round(clicked), minimum, maximum));
}

function renameSelected() {
  const leaf = selectedLeaf();
  if (!leaf) return;
  const name = window.prompt("Compartment name (empty clears it)", leaf.compartment ?? "");
  if (name === null) return;
  ctx.actions.renameLeaf(leaf.path, name.trim() || null);
}

function mergeSelected() {
  const leaf = selectedLeaf();
  if (!leaf) return;
  ctx.actions.mergeLeaf(leaf.path);
}

/** The wall band width: `(outer - interior) / 2`. */
function interiorOffset() {
  const body = ctx.state.plan.body;
  return (body.outer_width_mm - body.interior_width_mm) / 2;
}

/** Client coordinates to SVG user units (the viewBox frame). */
function svgPoint(svg, event) {
  const rect = svg.getBoundingClientRect();
  const viewBox = svg.viewBox.baseVal;
  const scale = Math.min(rect.width / viewBox.width, rect.height / viewBox.height) || 1;
  const offsetX = (rect.width - viewBox.width * scale) / 2;
  const offsetY = (rect.height - viewBox.height * scale) / 2;
  return {
    x: (event.clientX - rect.left - offsetX) / scale + viewBox.x,
    y: (event.clientY - rect.top - offsetY) / scale + viewBox.y,
  };
}

/** Drags one divider with live integer millimetres, clamped to the legal
 * range derived from the standard's minimum leaf and the divider thickness. */
function startDrag(svg, element, event) {
  const { state } = ctx;
  const divider = state.plan.dividers.find(
    (candidate) => candidate.path === element.dataset.divider,
  );
  if (!divider) return;

  const standard = state.standard;
  const minimum = standard.dividers.min_leaf_mm;
  const horizontal = divider.axis === "x";
  const parent = divider.parent;
  const size = horizontal ? parent.width_mm : parent.depth_mm;
  const maximum = Math.max(minimum, Math.round(size - minimum - standard.dividers.thickness_mm));
  const origin = horizontal ? parent.x_mm : parent.y_mm;
  const wall = interiorOffset();
  const startAt = Math.round(horizontal ? divider.x_mm - origin : divider.y_mm - origin);
  let current = startAt;
  let moved = false;

  try {
    element.setPointerCapture(event.pointerId);
  } catch {
    // Pointer capture is a nicety; the listeners below work without it.
  }
  element.classList.add("dragging");

  const label = document.createElementNS(SVG_NS, "text");
  label.setAttribute("class", "drag-label");
  label.setAttribute("font-size", String((Math.max(state.plan.body.outer_width_mm, state.plan.body.outer_depth_mm) / 40) * 1.3));
  svg.append(label);

  const update = (moveEvent) => {
    moved = true;
    const point = svgPoint(svg, moveEvent);
    const raw = horizontal ? point.x - wall - origin : point.y - wall - origin;
    current = clamp(Math.round(raw), minimum, maximum);
    if (horizontal) element.setAttribute("x", String(wall + origin + current));
    else element.setAttribute("y", String(wall + origin + current));
    label.setAttribute("x", String(point.x));
    label.setAttribute("y", String(point.y - 1));
    label.textContent = `${current} mm`;
  };

  const finish = () => {
    element.removeEventListener("pointermove", update);
    element.removeEventListener("pointerup", finish);
    element.removeEventListener("pointercancel", finish);
    element.classList.remove("dragging");
    label.remove();
    if (moved) {
      if (current !== startAt) ctx.actions.setDividerAt(divider.path, current);
      ctx.actions.selectDivider(divider.path);
    }
  };

  element.addEventListener("pointermove", update);
  element.addEventListener("pointerup", finish);
  element.addEventListener("pointercancel", finish);
}
