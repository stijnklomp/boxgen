// The SVG division editor: the plan view, compartment selection, split
// (toolbar buttons and side drags), the wall-side Add dividers chooser, rename,
// merge, and divider drags.
//
// Every edit is a design mutation that app.js sends to `/api/plan`; the
// editor never decides validity and owns no schema or SYS-2.0 rule. The local
// arithmetic only places what the plan already decided: the *drag and split
// clamp* derives the legal integer range from `/api/standard` (minimum
// compartment and divider thickness) and the parent rectangle the plan
// response carries, so a pointer can never leave the SYS-2.0 model; the
// even-divide preview draws the plan's own divider rectangles and its caption
// takes the span range straight from the plan's spans. The server still
// validates every committed edit (ADR-0002).

import * as panels from "./panels.js";

let ctx = null;

/** The one open **Add dividers** chooser, or null. */
let divideChooser = null;

/** Wires the editor to the app store and actions. */
export function setEditorContext(context) {
  ctx = context;
}

/** Closes the wall-side chooser; app.js calls this from every render so an
 * open chooser can never apply a stale plan. */
export function closeDivideChooser() {
  if (!divideChooser) return;
  const { element, controller } = divideChooser;
  clearDividePreview();
  divideChooser = null;
  controller.abort();
  element.remove();
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

/** The side drag handle band, in millimetres (inside the compartment). */
const HANDLE_MM = 2.5;

/** A compartment can split only when both children keep the minimum leaf. */
function canSplit(size) {
  const standard = ctx.state.standard;
  return size >= 2 * standard.dividers.min_leaf_mm + standard.dividers.thickness_mm;
}

/** The legal integer split range: millimetres from the compartment's origin
 * edge to the near divider face, both children at least the minimum leaf.
 * The maximum is floored: the far child starts one thickness later. */
function splitBounds(size) {
  const standard = ctx.state.standard;
  const minimum = standard.dividers.min_leaf_mm;
  const maximum = Math.max(minimum, Math.floor(size - minimum - standard.dividers.thickness_mm));
  return { minimum, maximum };
}

/** The magnetic snap targets: the divider centreline at a quarter, half, and
 * three quarters of the parent span. The stored position is the near face,
 * so the near face sits half a divider thickness before that centreline. */
const SNAP_FRACTIONS = [0.25, 0.5, 0.75];

/** How close, in screen pixels, the pointer must be to a snap target. */
const SNAP_RADIUS_PX = 12;

/** How far the dashed snap guide runs past the compartment, in millimetres. */
const SNAP_GUIDE_MARGIN_MM = 3;

/** The legal quarter targets, with their percentage labels. */
function snapTargets(size) {
  const standard = ctx.state.standard;
  const thickness = standard.dividers.thickness_mm;
  const { minimum, maximum } = splitBounds(size);
  const targets = [];
  for (const fraction of SNAP_FRACTIONS) {
    const at = Math.round(size * fraction - thickness / 2);
    if (at < minimum || at > maximum) continue;
    if (targets.some((target) => target.at === at)) continue;
    targets.push({ at, percent: Math.round(fraction * 100) });
  }
  return targets;
}

/** Resolves one pointer position to the near-face millimetres. While Shift is
 * held, the nearest legal quarter target within the snap radius wins;
 * otherwise the position is clamped to the legal integer range. */
function resolveDragAt(raw, size, snapRadiusMm, snapping) {
  const { minimum, maximum } = splitBounds(size);
  const clamped = clamp(Math.round(raw), minimum, maximum);
  if (!snapping) return { at: clamped, percent: null };
  let best = null;
  for (const target of snapTargets(size)) {
    const distance = Math.abs(raw - target.at);
    if (distance > snapRadiusMm) continue;
    if (!best || distance < best.distance) best = { ...target, distance };
  }
  return best ? { at: best.at, percent: best.percent } : { at: clamped, percent: null };
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
        <p><button type="button" class="btn primary" data-action="start-preset">Start from the standard preset</button></p>
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
    ${plan && !plan.valid ? `<div class="errors"><p>The spec violates the SYS-2.0 rules; saving and building are blocked until it is valid.</p>${errorList(plan.errors)}</div>` : ""}
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
    parts.push(...edgeHandlesHtml(leaf, wall));
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

/** The four side handles of one compartment: left/right edges work on the x
 * axis, top/bottom on y. Dragging adds one divider; a click on a wall side
 * opens the even-divide chooser. They sit inside the compartment and clear the
 * corners so every hit is unambiguous. */
function edgeHandlesHtml(leaf, wall) {
  const parts = [];
  const x = wall + leaf.x_mm;
  const y = wall + leaf.y_mm;
  const width = leaf.width_mm;
  const depth = leaf.depth_mm;
  const t = HANDLE_MM;
  if (canSplit(width) && depth > 2 * t) {
    parts.push(handleHtml(leaf, "x", "near", x, y + t, t, depth - 2 * t));
    parts.push(handleHtml(leaf, "x", "far", x + width - t, y + t, t, depth - 2 * t));
  }
  if (canSplit(depth) && width > 2 * t) {
    parts.push(handleHtml(leaf, "y", "near", x + t, y, width - 2 * t, t));
    parts.push(handleHtml(leaf, "y", "far", x + t, y + depth - t, width - 2 * t, t));
  }
  return parts;
}

/** One side band; `side` is `near` (origin/-x/-y) or `far` (+x/+y). */
function handleHtml(leaf, axis, side, x, y, width, height) {
  return `<rect class="edge-handle" data-edge-handle data-axis="${axis}" data-edge-side="${side}" data-edge-leaf="${escapeHtml(leaf.path)}" x="${x}" y="${y}" width="${width}" height="${height}"/>`;
}

/** The selection toolbar below the plan. */
function selectionToolbarHtml() {
  const { state } = ctx;
  const plan = state.plan;
  const selection = state.selection;
  if (!plan) return "";

  if (!selection) {
    return `<div class="selection-toolbar"><span class="hint">Click a compartment to select it; click a wall side to open Add dividers; a divider-side click does nothing; drag from any side to add one divider at the release position; drag a divider or click it to set an exact position. Hold Shift while dragging to snap to 25%, 50%, or 75%.</span></div>`;
  }

  if (selection.kind === "leaf") {
    const leaf = plan.leaves.find((candidate) => candidate.path === selection.path);
    if (!leaf) return "";
    const standard = state.standard;
    const minimum = 2 * standard.dividers.min_leaf_mm + standard.dividers.thickness_mm;
    const canSplitX = canSplit(leaf.width_mm);
    const canSplitY = canSplit(leaf.depth_mm);
    return `
    <div class="selection-toolbar">
      <span class="title">Compartment ${leaf.compartment ? `“${escapeHtml(leaf.compartment)}”` : "(unnamed)"}</span>
      <span class="range">${mm(leaf.width_mm)} × ${mm(leaf.depth_mm)} mm</span>
      <button type="button" class="btn small" data-editor="split-x" ${canSplitX ? "" : "disabled"} title="Split at the clicked position (x axis)">Split ↔ x</button>
      <button type="button" class="btn small" data-editor="split-y" ${canSplitY ? "" : "disabled"} title="Split at the clicked position (y axis)">Split ↕ y</button>
      <button type="button" class="btn small" data-editor="rename">Rename</button>
      <button type="button" class="btn small danger" data-editor="merge">Merge away</button>
      ${canSplitX && canSplitY ? "" : `<span class="range">A compartment needs ${minimum} mm to split.</span>`}
      ${canSplitX || canSplitY ? `<span class="range">Hold Shift while side-dragging to snap the new divider to 25 / 50 / 75%.</span>` : ""}
    </div>`;
  }

  const divider = plan.dividers.find((candidate) => candidate.path === selection.path);
  if (!divider) return "";
  const size = divider.axis === "x" ? divider.parent.width_mm : divider.parent.depth_mm;
  const { minimum, maximum } = splitBounds(size);
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
    <span class="range">legal range ${minimum}–${maximum} mm from the compartment edge; hold Shift to snap to 25 / 50 / 75%</span>
  </div>`;
}

/** Binds the plan: selection clicks and divider drags. */
function bindPlan(root) {
  const svg = root.querySelector("#plan");
  if (!svg) return;

  svg.addEventListener("click", (event) => {
    // A side press is a drag or the wall-side chooser; it never doubles as a
    // selection.
    if (event.target.closest("[data-edge-handle]")) return;
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
    if (divider) {
      startDrag(svg, divider, event);
      return;
    }
    const handle = event.target.closest("[data-edge-handle]");
    if (handle) startSplitDrag(svg, handle, event);
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
  const horizontal = axis === "x";
  const size = horizontal ? leaf.width_mm : leaf.depth_mm;
  const { minimum, maximum } = splitBounds(size);
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

/** Pixels per SVG user unit: the viewBox is fitted with `meet`. */
function svgScale(svg, rect = svg.getBoundingClientRect()) {
  const viewBox = svg.viewBox.baseVal;
  return Math.min(rect.width / viewBox.width, rect.height / viewBox.height) || 1;
}

/** Client coordinates to SVG user units (the viewBox frame). */
function svgPoint(svg, event) {
  const rect = svg.getBoundingClientRect();
  const viewBox = svg.viewBox.baseVal;
  const scale = svgScale(svg, rect);
  const offsetX = (rect.width - viewBox.width * scale) / 2;
  const offsetY = (rect.height - viewBox.height * scale) / 2;
  return {
    x: (event.clientX - rect.left - offsetX) / scale + viewBox.x,
    y: (event.clientY - rect.top - offsetY) / scale + viewBox.y,
  };
}

/** Drags one divider with live integer millimetres, clamped to the legal
 * range derived from the standard's minimum leaf and the divider thickness.
 * Holding Shift adds magnetic snap targets at the quarter positions. */
function startDrag(svg, element, event) {
  const { state } = ctx;
  const divider = state.plan.dividers.find(
    (candidate) => candidate.path === element.dataset.divider,
  );
  if (!divider) return;

  const horizontal = divider.axis === "x";
  const parent = divider.parent;
  const size = horizontal ? parent.width_mm : parent.depth_mm;
  const thickness = state.standard.dividers.thickness_mm;
  const origin = horizontal ? parent.x_mm : parent.y_mm;
  const wall = interiorOffset();
  const startAt = Math.round(horizontal ? divider.x_mm - origin : divider.y_mm - origin);
  const snapRadiusMm = SNAP_RADIUS_PX / svgScale(svg);
  let current = startAt;
  let percent = null;
  let moved = false;

  try {
    element.setPointerCapture(event.pointerId);
  } catch {
    // Pointer capture is a nicety; the listeners below work without it.
  }
  element.classList.add("dragging");

  const guide = document.createElementNS(SVG_NS, "line");
  guide.setAttribute("class", "snap-guide");
  guide.setAttribute("visibility", "hidden");
  svg.append(guide);

  const label = document.createElementNS(SVG_NS, "text");
  label.setAttribute("class", "drag-label");
  label.setAttribute("font-size", String((Math.max(state.plan.body.outer_width_mm, state.plan.body.outer_depth_mm) / 40) * 1.3));
  svg.append(label);

  const draw = (point) => {
    if (horizontal) element.setAttribute("x", String(wall + origin + current));
    else element.setAttribute("y", String(wall + origin + current));
    if (percent === null) {
      guide.setAttribute("visibility", "hidden");
    } else {
      const centre = wall + origin + current + thickness / 2;
      if (horizontal) {
        guide.setAttribute("x1", String(centre));
        guide.setAttribute("x2", String(centre));
        guide.setAttribute("y1", String(wall + parent.y_mm - SNAP_GUIDE_MARGIN_MM));
        guide.setAttribute("y2", String(wall + parent.y_mm + parent.depth_mm + SNAP_GUIDE_MARGIN_MM));
      } else {
        guide.setAttribute("x1", String(wall + parent.x_mm - SNAP_GUIDE_MARGIN_MM));
        guide.setAttribute("x2", String(wall + parent.x_mm + parent.width_mm + SNAP_GUIDE_MARGIN_MM));
        guide.setAttribute("y1", String(centre));
        guide.setAttribute("y2", String(centre));
      }
      guide.setAttribute("visibility", "visible");
    }
    if (point) {
      label.setAttribute("x", String(point.x));
      label.setAttribute("y", String(point.y - 1));
    }
    label.textContent = percent === null ? `${current} mm` : `${current} mm · ${percent}%`;
  };

  const apply = (raw, snapping, point) => {
    const resolved = resolveDragAt(raw, size, snapRadiusMm, snapping);
    current = resolved.at;
    percent = resolved.percent;
    draw(point);
  };

  const update = (moveEvent) => {
    moved = true;
    const point = svgPoint(svg, moveEvent);
    const raw = horizontal ? point.x - wall - origin : point.y - wall - origin;
    apply(raw, moveEvent.shiftKey, point);
  };

  const finish = (event) => {
    element.removeEventListener("pointermove", update);
    element.removeEventListener("pointerup", finish);
    element.removeEventListener("pointercancel", finish);
    element.classList.remove("dragging");
    if (moved && event?.type === "pointerup") {
      const point = svgPoint(svg, event);
      const raw = horizontal ? point.x - wall - origin : point.y - wall - origin;
      apply(raw, event.shiftKey, point);
    }
    guide.remove();
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

/** The plan's even-divide block for one leaf and axis, or null. */
function divideBlock(leaf, axis) {
  const block = leaf?.divide?.[axis] ?? null;
  return block && Array.isArray(block.options) && block.options.length > 0 ? block : null;
}

/** True when the clicked side lies on an inner wall face. */
function isWallSide(block, side) {
  return side === "near" ? block.near_wall === true : block.far_wall === true;
}

/** The neutral caption until a count is hovered or focused. */
const DIVIDE_CAPTION_IDLE = "Hover a count to preview";

/** The caption for one option: its count, compartment count, and span range.
 * A warned option names the warning threshold from the plan; the editor never
 * recomputes the warning from the spans or the perpendicular dimension. */
function divideCaption(option) {
  const count = option.count;
  const word = count === 1 ? "divider" : "dividers";
  const spans = (option.spans_mm ?? []).filter((value) => Number.isFinite(value));
  let caption = `${count} ${word} · ${count + 1} compartments`;
  if (spans.length > 0) {
    const minimum = Math.min(...spans);
    const maximum = Math.max(...spans);
    caption += ` · ${mm(minimum)}–${mm(maximum)} mm each`;
  }
  if (option.warned) {
    caption += ` · below the ${mm(ctx.state.standard.dividers.warn_leaf_mm)} mm warning threshold`;
  }
  return caption;
}

/** Removes every preview line and restores the neutral caption. */
function clearDividePreview() {
  const svg = document.getElementById("plan");
  if (svg) {
    for (const line of svg.querySelectorAll(".divide-preview-line")) line.remove();
  }
  const caption = divideChooser?.caption;
  if (!caption) return;
  caption.textContent = DIVIDE_CAPTION_IDLE;
  caption.classList.remove("warned");
}

/** Draws one option's divider rectangles, verbatim from the plan, above the
 * leaves and dividers, and shows its caption. No request and no state change:
 * the preview disappears when the pointer or focus leaves the count. */
function previewDivideOption(option) {
  clearDividePreview();
  const svg = document.getElementById("plan");
  if (!svg || !ctx.state.plan?.body) return;
  const wall = interiorOffset();
  for (const divider of option.dividers ?? []) {
    const line = document.createElementNS(SVG_NS, "rect");
    line.setAttribute("class", "divide-preview-line");
    line.setAttribute("x", String(wall + divider.x_mm));
    line.setAttribute("y", String(wall + divider.y_mm));
    line.setAttribute("width", String(divider.width_mm));
    line.setAttribute("height", String(divider.depth_mm));
    svg.append(line);
  }
  const caption = divideChooser?.caption;
  if (!caption) return;
  caption.textContent = divideCaption(option);
  caption.classList.toggle("warned", option.warned === true);
}

/** Opens the anchored, non-modal Add dividers chooser for one wall side.
 * Every count, span, and the replacement subtree comes from the plan; this
 * function only renders them. */
function openDivideChooser(path, axis, side, pointer) {
  const { state } = ctx;
  const leaf = state.plan?.leaves?.find((candidate) => candidate.path === path) ?? null;
  const block = divideBlock(leaf, axis);
  if (!leaf || !block || !isWallSide(block, side)) return;

  closeDivideChooser();

  const controller = new AbortController();
  const element = document.createElement("div");
  element.className = "divide-chooser";
  element.setAttribute("role", "dialog");
  element.setAttribute("aria-label", "Add dividers");
  element.style.left = `${pointer.clientX + 8}px`;
  element.style.top = `${pointer.clientY + 8}px`;

  const header = document.createElement("div");
  header.className = "divide-chooser-header";
  const title = document.createElement("span");
  title.className = "title";
  title.textContent = leaf.compartment ? `Compartment “${leaf.compartment}”` : "Compartment";
  const span = document.createElement("span");
  span.className = "range";
  span.textContent =
    axis === "x" ? `${mm(leaf.width_mm)} mm wide` : `${mm(leaf.depth_mm)} mm deep`;
  header.append(title, span);

  const grid = document.createElement("div");
  grid.className = "divide-count-grid";
  for (const option of block.options) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "divide-count";
    button.textContent = String(option.count);
    const label = option.count === 1 ? "Add one divider" : `Add ${option.count} dividers`;
    const warning = `below the ${mm(state.standard.dividers.warn_leaf_mm)} mm warning`;
    button.title = option.warned ? `${label} (${warning})` : label;
    button.setAttribute("aria-label", button.title);
    if (option.warned) {
      button.classList.add("warned");
      const dot = document.createElement("span");
      dot.className = "divide-warn-dot";
      dot.setAttribute("aria-hidden", "true");
      button.append(dot);
    }
    const show = () => previewDivideOption(option);
    button.addEventListener("pointerenter", show);
    button.addEventListener("pointerleave", clearDividePreview);
    button.addEventListener("focus", show);
    button.addEventListener("blur", clearDividePreview);
    button.addEventListener("click", () => {
      closeDivideChooser();
      ctx.actions.applyDivide(leaf.path, option.division);
    });
    grid.append(button);
  }
  const caption = document.createElement("p");
  caption.className = "divide-caption";
  caption.textContent = DIVIDE_CAPTION_IDLE;
  element.append(header, grid, caption);
  document.body.append(element);

  // Measure first, then keep the panel inside the viewport.
  const margin = 8;
  const panel = element.getBoundingClientRect();
  const left = Math.max(
    margin,
    Math.min(pointer.clientX + 8, window.innerWidth - panel.width - margin),
  );
  const top = Math.max(
    margin,
    Math.min(pointer.clientY + 8, window.innerHeight - panel.height - margin),
  );
  element.style.left = `${left}px`;
  element.style.top = `${top}px`;

  document.addEventListener(
    "keydown",
    (event) => {
      if (event.key === "Escape") closeDivideChooser();
    },
    { signal: controller.signal },
  );
  document.addEventListener(
    "pointerdown",
    (event) => {
      if (!element.contains(event.target)) closeDivideChooser();
    },
    { signal: controller.signal },
  );
  window.addEventListener("scroll", closeDivideChooser, {
    capture: true,
    signal: controller.signal,
  });
  window.addEventListener("resize", closeDivideChooser, { signal: controller.signal });

  divideChooser = { element, controller, caption };
}

/** Drags from a compartment side to add one divider at the release position,
 * with the same legal clamp and Shift quarter snapping as a divider drag. A
 * no-move release is a click: on a wall side it opens the Add dividers
 * chooser, and on a divider side it does nothing. The preview stays hidden
 * until the pointer crosses the slop, so that click never flashes a divider at
 * the middle position. */
function startSplitDrag(svg, handle, event) {
  const { state } = ctx;
  const leaf = state.plan.leaves.find((candidate) => candidate.path === handle.dataset.edgeLeaf);
  if (!leaf) return;

  const horizontal = handle.dataset.axis === "x";
  const axis = horizontal ? "x" : "y";
  const side = handle.dataset.edgeSide === "far" ? "far" : "near";
  const size = horizontal ? leaf.width_mm : leaf.depth_mm;
  if (!canSplit(size)) return;

  const { minimum, maximum } = splitBounds(size);
  const thickness = state.standard.dividers.thickness_mm;
  const wall = interiorOffset();
  const origin = horizontal ? leaf.x_mm : leaf.y_mm;
  const unit = Math.max(state.plan.body.outer_width_mm, state.plan.body.outer_depth_mm) / 40;

  const rect = svg.getBoundingClientRect();
  const scale = svgScale(svg, rect);
  const slop = 4 / scale; // movement below ~4 px along the split axis is still a click
  const snapRadiusMm = SNAP_RADIUS_PX / scale;

  const pointerAt = (moveEvent) => {
    const point = svgPoint(svg, moveEvent);
    return horizontal ? point.x - wall - origin : point.y - wall - origin;
  };
  const startAt = pointerAt(event);
  let moved = false;
  let current = clamp(Math.round((size - thickness) / 2), minimum, maximum);
  let percent = null;

  try {
    handle.setPointerCapture(event.pointerId);
  } catch {
    // Pointer capture is a nicety; the listeners below work without it.
  }
  handle.classList.add("dragging");

  const preview = document.createElementNS(SVG_NS, "rect");
  preview.setAttribute("class", "divider-preview");
  preview.setAttribute("width", String(horizontal ? thickness : leaf.width_mm));
  preview.setAttribute("height", String(horizontal ? leaf.depth_mm : thickness));
  preview.setAttribute("visibility", "hidden");
  svg.append(preview);

  const guide = document.createElementNS(SVG_NS, "line");
  guide.setAttribute("class", "snap-guide");
  guide.setAttribute("visibility", "hidden");
  svg.append(guide);

  const label = document.createElementNS(SVG_NS, "text");
  label.setAttribute("class", "drag-label");
  label.setAttribute("font-size", String(unit * 1.3));
  label.setAttribute("visibility", "hidden");
  svg.append(label);

  const draw = () => {
    if (horizontal) {
      preview.setAttribute("x", String(wall + origin + current));
      preview.setAttribute("y", String(wall + leaf.y_mm));
      label.setAttribute("x", String(wall + origin + current + thickness / 2));
      label.setAttribute("y", String(wall + leaf.y_mm + leaf.depth_mm / 2 + unit * 0.8));
    } else {
      preview.setAttribute("x", String(wall + leaf.x_mm));
      preview.setAttribute("y", String(wall + origin + current));
      label.setAttribute("x", String(wall + leaf.x_mm + leaf.width_mm / 2));
      label.setAttribute("y", String(wall + origin + current - unit * 0.8));
    }
    label.textContent = percent === null ? `${current} mm` : `${current} mm · ${percent}%`;
    if (percent === null) {
      guide.setAttribute("visibility", "hidden");
    } else {
      const centre = wall + origin + current + thickness / 2;
      if (horizontal) {
        guide.setAttribute("x1", String(centre));
        guide.setAttribute("x2", String(centre));
        guide.setAttribute("y1", String(wall + leaf.y_mm - SNAP_GUIDE_MARGIN_MM));
        guide.setAttribute("y2", String(wall + leaf.y_mm + leaf.depth_mm + SNAP_GUIDE_MARGIN_MM));
      } else {
        guide.setAttribute("x1", String(wall + leaf.x_mm - SNAP_GUIDE_MARGIN_MM));
        guide.setAttribute("x2", String(wall + leaf.x_mm + leaf.width_mm + SNAP_GUIDE_MARGIN_MM));
        guide.setAttribute("y1", String(centre));
        guide.setAttribute("y2", String(centre));
      }
      guide.setAttribute("visibility", "visible");
    }
  };

  const apply = (raw, snapping) => {
    const resolved = resolveDragAt(raw, size, snapRadiusMm, snapping);
    current = resolved.at;
    percent = resolved.percent;
    draw();
  };

  const show = () => {
    preview.setAttribute("visibility", "visible");
    label.setAttribute("visibility", "visible");
  };

  const update = (moveEvent) => {
    const raw = pointerAt(moveEvent);
    if (!moved && Math.abs(raw - startAt) >= slop) moved = true;
    if (!moved) return;
    show();
    apply(raw, moveEvent.shiftKey);
  };

  const finish = (commit, finishEvent) => {
    handle.removeEventListener("pointermove", update);
    handle.removeEventListener("pointerup", onUp);
    handle.removeEventListener("pointercancel", onCancel);
    handle.classList.remove("dragging");
    const dragged = moved;
    if (commit && dragged && finishEvent) apply(pointerAt(finishEvent), finishEvent.shiftKey);
    preview.remove();
    guide.remove();
    label.remove();
    if (!commit) return;
    if (dragged) {
      ctx.actions.splitLeaf(leaf.path, axis, current);
      return;
    }
    if (finishEvent) openDivideChooser(leaf.path, axis, side, finishEvent);
  };
  const onUp = (event) => finish(true, event);
  const onCancel = () => finish(false, null);

  handle.addEventListener("pointermove", update);
  handle.addEventListener("pointerup", onUp);
  handle.addEventListener("pointercancel", onCancel);
}
