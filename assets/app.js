// The app: store, view routing, undo/redo, drafts, and the build/export flow.
//
// The store holds one spec at a time. Every edit runs through `mutate`, which
// posts the whole design to `/api/plan` and takes the Rust validator's answer
// (ADR-0002: the UI owns no rules). Save is only offered when the plan is
// valid; build, export, publish, and preview share the one build job.
//
// Drafts autosave to `localStorage` per box id; the calibrated preset is
// 4 × 4 · 6U · lid + ribs.

import { api, ApiError } from "./api.js";
import { renderEditorView, setEditorContext } from "./editor.js";
import { renderStandardView, setPanelsContext } from "./panels.js";
import { mountViewer } from "./viewer3d.js";

/** The calibrated preset every new box starts from. */
const PRESET = Object.freeze({
  footprint: [4, 4],
  height_u: 6,
  lid: true,
  lid_ribs: true,
});

const UNDO_LIMIT = 200;
const POLL_MS = 400;
const POLL_LIMIT = 3000; // about 20 minutes
const DRAFT_PREFIX = "boxgen.draft.";
const PREVIEW_PREFIX = "boxgen.preview.";

export const state = {
  view: "boxes",
  key: null, // registry key used in URLs (folder name or id)
  id: null, // spec id used by the API (build, export, publish)
  design: null,
  savedText: null,
  openErrors: [],
  openWarnings: [],
  revision: 0,
  savedRevision: 0,
  restoredDraft: false,
  plan: null,
  planError: null,
  planSeq: 0,
  standard: null,
  standardError: null,
  standardLoading: false,
  systemVersion: null,
  boxes: [],
  boxesError: null,
  selection: null,
  undo: [],
  redo: [],
  build: { state: "idle" },
  preview: null,
  exportResult: null,
  publishResult: null,
  indexResult: null,
  toast: null,
};

let toastTimer = null;

// ------------------------------------------------------------- helpers

function clone(value) {
  return structuredClone(value);
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** A canonical JSON string (sorted keys), for draft/disk comparisons. */
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.keys(value)
      .sort()
      .map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value ?? null);
}

function designsEqual(left, right) {
  return canonical(left) === canonical(right);
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function dirty() {
  return Boolean(state.design) && state.revision !== state.savedRevision;
}

/** True when a 3D preview exists but shows an older spec revision. */
function previewStale() {
  const preview = state.preview;
  if (!preview || !state.design) return false;
  return preview.id !== state.id || (preview.fingerprint ?? null) !== canonical(state.design);
}

function numberOrNull(value) {
  return Number.isFinite(value) ? value : null;
}

function presetDesign(id) {
  return {
    id,
    footprint: [...PRESET.footprint],
    height_u: PRESET.height_u,
    lid: PRESET.lid,
    lid_ribs: PRESET.lid_ribs,
    notes: null,
    contents: null,
    divisions: { type: "leaf", compartment: null },
  };
}

// -------------------------------------------------------------- drafts

function draftKey(id) {
  return `${DRAFT_PREFIX}${id}`;
}

function readDraft(id) {
  try {
    const text = localStorage.getItem(draftKey(id));
    return text ? JSON.parse(text) : null;
  } catch {
    return null;
  }
}

function writeDraft() {
  if (!state.id || !state.design) return;
  try {
    localStorage.setItem(
      draftKey(state.id),
      JSON.stringify({
        design: state.design,
        revision: state.revision,
        savedRevision: state.savedRevision,
        savedText: state.savedText,
        updatedAt: Date.now(),
      }),
    );
  } catch {
    // Drafts are best effort; a disabled localStorage must not break editing.
  }
}

function clearDraft(id) {
  try {
    localStorage.removeItem(draftKey(id));
  } catch {
    // ignore
  }
}

function readPreview(id) {
  try {
    const text = localStorage.getItem(`${PREVIEW_PREFIX}${id}`);
    return text ? JSON.parse(text) : null;
  } catch {
    return null;
  }
}

function writePreview() {
  if (!state.id) return;
  try {
    localStorage.setItem(
      `${PREVIEW_PREFIX}${state.id}`,
      JSON.stringify({ id: state.id, revision: state.revision, at: Date.now() }),
    );
  } catch {
    // ignore
  }
}

// ------------------------------------------------------ design mutation

/** Replaces the design, pushing one undo step, and re-plans in Rust. */
function setDesign(next, { history = true } = {}) {
  if (!state.design) return;
  if (history) {
    state.undo.push(clone(state.design));
    if (state.undo.length > UNDO_LIMIT) state.undo.shift();
    state.redo = [];
  }
  state.design = next;
  state.revision += 1;
  writeDraft();
  refreshPlan();
}

/** Applies one in-place change to a clone of the design. */
function mutate(change) {
  if (!state.design) return;
  const next = clone(state.design);
  change(next);
  setDesign(next);
}

function applyHistory(next) {
  state.design = next;
  state.revision += 1;
  state.selection = null;
  writeDraft();
  refreshPlan();
}

function undo() {
  const previous = state.undo.pop();
  if (!previous) return;
  state.redo.push(clone(state.design));
  applyHistory(previous);
}

function redo() {
  const next = state.redo.pop();
  if (!next) return;
  state.undo.push(clone(state.design));
  applyHistory(next);
}

/** Asks the Rust validator; the answer is the only source of validity. */
async function refreshPlan() {
  if (!state.design) {
    state.plan = null;
    render();
    return;
  }
  const seq = ++state.planSeq;
  try {
    const plan = await api.plan(state.design);
    if (seq !== state.planSeq) return;
    state.plan = plan;
    state.planError = null;
  } catch (error) {
    if (seq !== state.planSeq) return;
    state.plan = null;
    state.planError = error;
  }
  render();
}

// ----------------------------------------------------------- selection

function selectLeaf(path, at) {
  state.selection = { kind: "leaf", path, at };
  render();
}

function selectDivider(path) {
  state.selection = { kind: "divider", path };
  render();
}

/** Splits one compartment; the near child keeps its name, like `divide`. */
function splitLeaf(path, axis, at) {
  mutate((design) => {
    const node = nodeAtPath(design.divisions, path);
    if (!node || node.type !== "leaf") return;
    const name = node.compartment ?? null;
    node.type = "split";
    node.axis = axis;
    node.at_mm = at;
    node.near = { type: "leaf", compartment: name };
    node.far = { type: "leaf", compartment: null };
    delete node.compartment;
  });
  state.selection = null;
}

function renameLeaf(path, name) {
  mutate((design) => {
    const node = nodeAtPath(design.divisions, path);
    if (node && node.type === "leaf") node.compartment = name;
  });
}

/** Merges a compartment away: the parent split becomes the sibling subtree.
 * When both compartments are named, the surviving name is asked for. */
async function mergeLeaf(path) {
  const steps = pathSteps(path);
  if (steps.length === 0) {
    toast("The whole box is one compartment; there is nothing to merge.", { error: true });
    return;
  }
  const parentPath = `divisions${steps
    .slice(0, -1)
    .map((step) => `.${step}`)
    .join("")}`;
  const side = steps[steps.length - 1];
  const parent = nodeAtPath(state.design.divisions, parentPath);
  if (!parent || parent.type !== "split") return;
  const leaf = side === "near" ? parent.near : parent.far;
  const sibling = side === "near" ? parent.far : parent.near;

  let keep = undefined;
  if (
    leaf.type === "leaf" &&
    leaf.compartment &&
    sibling.type === "leaf" &&
    sibling.compartment
  ) {
    const choice = await choose("Both compartments are named. Which name survives?", [
      { label: `Keep “${leaf.compartment}”`, value: "leaf" },
      { label: `Keep “${sibling.compartment}”`, value: "sibling" },
      { label: "Keep no name", value: "none" },
    ]);
    if (choice === null) return;
    keep = choice;
  }

  mutate((design) => {
    const parent = nodeAtPath(design.divisions, parentPath);
    if (!parent || parent.type !== "split") return;
    const sibling = side === "near" ? parent.far : parent.near;
    const replacement = clone(sibling);
    if (keep !== undefined && replacement.type === "leaf") {
      replacement.compartment =
        keep === "leaf" ? leaf.compartment : keep === "sibling" ? sibling.compartment : null;
    }
    if (parentPath === "divisions") design.divisions = replacement;
    else replaceNodeAtPath(design.divisions, parentPath, replacement);
  });
  state.selection = null;
}

function setDividerAt(path, at) {
  mutate((design) => {
    const node = nodeAtPath(design.divisions, path);
    if (node && node.type === "split") node.at_mm = at;
  });
}

function pathSteps(path) {
  return path.split(".").slice(1);
}

function nodeAtPath(root, path) {
  let node = root;
  for (const step of pathSteps(path)) {
    if (!node || typeof node !== "object") return null;
    node = node[step];
  }
  return node ?? null;
}

function replaceNodeAtPath(root, path, replacement) {
  const steps = pathSteps(path);
  let parent = root;
  for (const step of steps.slice(0, -1)) parent = parent[step];
  parent[steps[steps.length - 1]] = replacement;
}

/** A small modal choice; resolves the picked value, or null on cancel. */
function choose(title, options) {
  return new Promise((resolve) => {
    let settled = false;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      resolve(value);
    };
    const dialog = document.createElement("dialog");
    dialog.className = "choose-dialog";
    const heading = document.createElement("h3");
    heading.textContent = title;
    dialog.append(heading);
    const list = document.createElement("div");
    list.className = "options";
    for (const option of options) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "btn";
      button.textContent = option.label;
      button.addEventListener("click", () => {
        finish(option.value);
        dialog.close();
      });
      list.append(button);
    }
    dialog.append(list);
    dialog.addEventListener("close", () => {
      finish(null);
      dialog.remove();
    });
    document.body.append(dialog);
    dialog.showModal();
  });
}

/** One click from the fit assistant: apply the combo and record the item. */
function applyFitCandidate(candidate, item) {
  mutate((design) => {
    design.footprint = [...candidate.footprint];
    design.height_u = candidate.height_u;
    design.contents = {
      description: item.description ?? null,
      soft: Boolean(item.soft),
      max_length_mm: numberOrNull(item.length_mm),
      max_width_mm: numberOrNull(item.width_mm),
      max_height_mm: numberOrNull(item.height_mm),
      weight_kg: numberOrNull(item.weight_kg),
    };
  });
  toast(
    `Applied ${candidate.footprint[0]} × ${candidate.footprint[1]} · ${candidate.height_u}U and recorded the contents.`,
  );
}

// ------------------------------------------------------- registry flow

async function refreshBoxes() {
  try {
    const response = await api.boxes();
    state.boxes = response.boxes;
    state.boxesError = null;
  } catch (error) {
    state.boxesError = error;
  }
  render();
}

async function openBox(key) {
  state.key = key;
  try {
    const opened = await api.open(key);
    state.id = opened.id || key;
    state.savedText = opened.text;
    state.openErrors = opened.valid ? [] : (opened.errors ?? []);
    state.openWarnings = opened.warnings ?? [];
    state.selection = null;
    state.undo = [];
    state.redo = [];
    state.build = { state: "idle" };
    state.exportResult = null;
    state.publishResult = null;
    state.indexResult = null;

    const savedDesign = opened.design ?? null;
    const draft = readDraft(state.id);
    if (draft && savedDesign && !designsEqual(draft.design, savedDesign)) {
      state.design = draft.design;
      state.revision = draft.revision ?? 1;
      state.savedRevision = draft.savedRevision ?? 0;
      state.restoredDraft = true;
    } else if (savedDesign) {
      state.design = savedDesign;
      state.revision = 0;
      state.savedRevision = 0;
      state.restoredDraft = false;
      if (draft) clearDraft(state.id);
    } else if (draft) {
      state.design = draft.design;
      state.revision = draft.revision ?? 1;
      state.savedRevision = draft.savedRevision ?? 0;
      state.restoredDraft = true;
    } else {
      state.design = null;
      state.revision = 0;
      state.savedRevision = 0;
      state.restoredDraft = false;
    }
    state.preview = readPreview(state.id);
    await refreshPlan();
  } catch (error) {
    showApiError(error);
    navigate("#/boxes");
  }
}

async function newBoxAction() {
  const answer = window.prompt(
    "New box id (lowercase kebab-case or <N>x<M>-<height>U-<slug>-<rev>)",
    "1x1-6U-new-box-v1",
  );
  if (answer === null) return;
  const id = answer.trim();
  if (!id) return;
  state.key = id;
  state.id = id;
  state.design = presetDesign(id);
  state.revision = 1;
  state.savedRevision = 0;
  state.restoredDraft = false;
  state.savedText = null;
  state.openErrors = [];
  state.openWarnings = [];
  state.selection = null;
  state.undo = [];
  state.redo = [];
  state.build = { state: "idle" };
  state.preview = null;
  state.exportResult = null;
  state.publishResult = null;
  state.indexResult = null;
  writeDraft();
  toast("New box starts from the calibrated preset: 4 × 4 · 6U · lid · ribs.");
  navigate(`#/box/${encodeURIComponent(id)}`);
  refreshPlan();
}

async function openBoxAction(key) {
  navigate(`#/box/${encodeURIComponent(key)}`);
}

async function duplicateBoxAction(key) {
  try {
    const opened = await api.open(key);
    if (!opened.design) {
      toast("This spec does not parse; open it and start from the preset instead.", {
        error: true,
      });
      return;
    }
    const answer = window.prompt("Duplicate as a new box id", `${opened.id}-v2`);
    if (answer === null) return;
    const id = answer.trim();
    if (!id || id === opened.id) return;
    const design = clone(opened.design);
    design.id = id;
    const saved = await api.save(id, design);
    toast(`Saved ${saved.path}.`);
    await refreshBoxes();
    navigate(`#/box/${encodeURIComponent(id)}`);
  } catch (error) {
    showApiError(error);
  }
}

function discardDraft() {
  clearDraft(state.id);
  toast("Draft discarded; reloading the spec from disk.");
  openBox(state.key);
}

function resetToPreset() {
  if (!state.design) return;
  setDesign(presetDesign(state.design.id));
  toast("Reset to the calibrated preset: 4 × 4 · 6U · lid · ribs.");
}

function startFromPreset() {
  const id = state.id ?? state.key;
  if (!id) return;
  state.design = presetDesign(id);
  state.revision = 1;
  state.savedRevision = 0;
  state.restoredDraft = false;
  state.openErrors = [];
  writeDraft();
  refreshPlan();
}

// ---------------------------------------------------------- save flow

/** Validates and writes `spec.yaml`; throws on any failure. */
async function save() {
  if (!state.design) throw new ApiError(422, "There is no spec to save.", []);
  const key = state.key ?? state.design.id;
  const saved = await api.save(key, state.design);
  state.savedRevision = state.revision;
  state.savedText = saved.text;
  state.openErrors = [];
  state.openWarnings = saved.warnings ?? [];
  writeDraft();
  return saved;
}

async function saveAction() {
  try {
    const saved = await save();
    toast(`Saved ${saved.path}.`);
    await refreshBoxes();
  } catch (error) {
    showApiError(error);
  }
  render();
}

async function saveAsAction() {
  if (!state.design) return;
  const answer = window.prompt(
    "Save as a new box id (the current box is kept)",
    `${state.design.id}-v2`,
  );
  if (answer === null) return;
  const id = answer.trim();
  if (!id || id === state.design.id) return;

  const design = clone(state.design);
  design.id = id;
  try {
    const saved = await api.save(id, design);
    state.key = id;
    state.id = id;
    state.design = design;
    state.savedRevision = state.revision;
    state.savedText = saved.text;
    state.openErrors = [];
    state.openWarnings = saved.warnings ?? [];
    state.restoredDraft = false;
    state.undo = [];
    state.redo = [];
    writeDraft();
    toast(`Saved ${saved.path}.`);
    await refreshBoxes();
    navigate(`#/box/${encodeURIComponent(id)}`);
  } catch (error) {
    showApiError(error);
  }
}

// --------------------------------------------------------- build flow

async function waitForIdle() {
  for (let attempt = 0; attempt < POLL_LIMIT; attempt += 1) {
    const status = await api.buildStatus();
    if (status.state !== "running") return status;
    await sleep(POLL_MS);
  }
  throw new ApiError(500, "The running build did not finish in time.", []);
}

async function waitForJob(revision) {
  for (let attempt = 0; attempt < POLL_LIMIT; attempt += 1) {
    const status = await api.buildStatus();
    if (status.state === "running") {
      state.build = {
        state: "running",
        id: status.id,
        stage: status.stage,
        elapsed_ms: status.elapsed_ms,
        revision,
      };
      render();
      await sleep(POLL_MS);
      continue;
    }
    if (status.state === "done") {
      const outcome = status.outcome;
      state.build = { state: "done", outcome, revision, id: outcome.id };
      if (outcome.id === state.id && revision === state.revision) {
        state.preview = {
          id: state.id,
          revision,
          fingerprint: canonical(state.design),
        };
        writePreview();
      }
      render();
      return outcome;
    }
    if (status.state === "failed") {
      state.build = {
        state: "failed",
        error: status.error,
        diagnostic: status.diagnostic,
        revision,
        id: state.id,
      };
      render();
      throw new ApiError(500, status.error, status.diagnostic ? [status.diagnostic] : []);
    }
    throw new ApiError(500, "The build job is no longer running; start it again.", []);
  }
  throw new ApiError(500, "The build did not finish in time.", []);
}

/** Starts the one build job for the current design and follows it. */
async function startBuild() {
  const design = clone(state.design);
  const revision = state.revision;
  try {
    await api.build(design);
  } catch (error) {
    if (error instanceof ApiError && error.status === 409) {
      await waitForIdle();
      await api.build(design);
    } else {
      throw error;
    }
  }
  state.build = { state: "running", id: design.id, stage: "preparing", elapsed_ms: 0, revision };
  render();
  return waitForJob(revision);
}

/** The build for the current revision, starting one when stale. */
async function ensureBuild() {
  if (
    state.build.state === "done" &&
    state.build.revision === state.revision &&
    state.build.outcome?.id === state.design?.id
  ) {
    return state.build.outcome;
  }
  return startBuild();
}

async function buildAction() {
  try {
    await ensureBuild();
    toast(`Build finished for ${state.id}.`);
  } catch (error) {
    showApiError(error);
  }
  render();
}

async function exportAction() {
  try {
    await ensureBuild();
    const result = await api.export(state.design.id);
    state.exportResult = result;
    toast(`Exported ${result.artifacts.length} file(s) to ${result.out_dir}.`);
  } catch (error) {
    showApiError(error);
  }
  render();
}

async function publishAction() {
  try {
    if (dirty()) await save();
    await ensureBuild();
    const result = await api.publish(state.design.id);
    state.publishResult = result;
    toast(`Published to ${result.published_dir}.`);
  } catch (error) {
    showApiError(error);
  }
  render();
}

async function indexAction() {
  try {
    const result = await api.index();
    state.indexResult = result;
    toast(`Wrote ${result.path} (${result.boxes} box${result.boxes === 1 ? "" : "es"}).`);
  } catch (error) {
    showApiError(error);
  }
  render();
}

async function previewAction() {
  try {
    await ensureBuild();
    state.preview = {
      id: state.id,
      revision: state.revision,
      fingerprint: canonical(state.design),
    };
    writePreview();
    navigate(`#/box/${encodeURIComponent(state.key ?? state.id)}/viewer`);
  } catch (error) {
    showApiError(error);
  }
}

async function viewerExportAction() {
  state.view = "editor";
  render();
  location.hash = editorHash();
  await exportAction();
}

function editorHash() {
  return `#/box/${encodeURIComponent(state.key ?? state.id)}`;
}

// ------------------------------------------------------------- toast

function toast(message, { error = false, errors = [] } = {}) {
  state.toast = { message, error, errors };
  renderToast();
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    state.toast = null;
    renderToast();
  }, 8000);
}

function renderToast() {
  const host = document.getElementById("toast-host");
  if (!host) return;
  const current = state.toast;
  host.innerHTML = current
    ? `<div class="toast${current.error ? " error" : ""}">${escapeHtml(current.message)}${
        current.errors.length > 0
          ? `<ul>${current.errors.map((item) => `<li>${escapeHtml(item)}</li>`).join("")}</ul>`
          : ""
      }</div>`
    : "";
}

function showApiError(error) {
  if (error instanceof ApiError) {
    toast(error.message, { error: true, errors: error.errors });
  } else {
    toast(String(error?.message ?? error), { error: true });
  }
}

async function copyAction(element) {
  const text = element.dataset.copy ?? "";
  try {
    await navigator.clipboard.writeText(text);
    toast(`Copied ${text}.`);
  } catch {
    toast(`Copy failed; the path is ${text}.`, { error: true });
  }
}

// ------------------------------------------------------------ standard

async function ensureStandard() {
  if (state.standard || state.standardLoading || state.standardError) return;
  state.standardLoading = true;
  try {
    state.standard = await api.standard();
  } catch (error) {
    state.standardError = error;
  }
  state.standardLoading = false;
  render();
}

// -------------------------------------------------------------- views

function render() {
  const app = document.getElementById("app");
  if (!app) return;
  const openDetails = new Set();
  app.querySelectorAll("details[data-key]").forEach((element) => {
    if (element.open) openDetails.add(element.dataset.key);
  });
  const scroll = window.scrollY;

  if (state.view === "editor") renderEditorView(app);
  else if (state.view === "viewer") renderViewerView(app);
  else if (state.view === "standard") renderStandardView(app);
  else renderBoxesView(app);

  app.querySelectorAll("details[data-key]").forEach((element) => {
    if (openDetails.has(element.dataset.key)) element.open = true;
  });
  window.scrollTo(0, scroll);
  renderHeader();
}

function renderHeader() {
  const element = document.getElementById("system-version");
  if (!element) return;
  const version = state.systemVersion ?? "";
  const calibration = state.standard?.calibration?.date;
  element.textContent = calibration ? `${version} · calibration ${calibration}` : version;
}

function renderBoxesView(root) {
  const rows = state.boxes.map(boxRowHtml).join("");
  root.innerHTML = `
  <section class="boxes-view">
    <div class="list-toolbar">
      <h1>Boxes</h1>
      <span class="spacer"></span>
      <button type="button" class="btn" data-action="refresh-boxes">Refresh</button>
      <button type="button" class="btn primary" data-action="new-box">New box</button>
    </div>
    ${state.boxesError ? `<div class="errors"><p>${escapeHtml(state.boxesError.message)}</p></div>` : ""}
    ${
      state.boxes.length === 0
        ? `<div class="empty">No boxes recorded yet. Create one with “New box”, or publish a build with <code>boxgen build --publish</code>.</div>`
        : `<div class="boxes">${rows}</div>`
    }
    <p class="hint">The list scans <code>boxes/*/spec.yaml</code>. A spec that does not parse is still listed; open it to see its errors.</p>
  </section>`;
  bindActions(root);
}

function boxRowHtml(box) {
  const footprint = box.footprint ? `${box.footprint[0]} × ${box.footprint[1]}` : "—";
  const height = box.height_u ? `${box.height_u}U` : "—";
  const lid =
    box.lid === null || box.lid === undefined
      ? "—"
      : box.lid
        ? box.lid_ribs
          ? "lid + ribs"
          : "lid"
        : "open";
  const folder = box.folder ?? box.id;
  return `
  <article class="box-row${box.valid ? "" : " invalid"}">
    <div>
      <div class="box-id">${escapeHtml(box.id)}</div>
      ${folder !== box.id ? `<div class="box-meta">folder: ${escapeHtml(folder)}</div>` : ""}
      <div class="box-meta">${escapeHtml(box.path)}</div>
    </div>
    <div class="box-meta">${footprint}</div>
    <div class="box-meta">${height}</div>
    <div class="box-meta">${lid}</div>
    <div class="box-errors">${box.valid ? "" : escapeHtml(box.errors?.[0] ?? "invalid spec")}</div>
    <div class="box-actions">
      <button type="button" class="btn small" data-action="open-box" data-box="${escapeHtml(folder)}">Open</button>
      <button type="button" class="btn small" data-action="duplicate-box" data-box="${escapeHtml(folder)}" ${box.valid ? "" : "disabled"}>Duplicate</button>
    </div>
  </article>`;
}

function renderViewerView(root) {
  const id = state.id;
  const preview = state.preview;
  const stale =
    !preview ||
    preview.id !== id ||
    (preview.fingerprint ?? null) !== canonical(state.design);
  const building = state.build.state === "running" && state.build.revision === state.revision;

  const toolbar = `
    <div class="viewer-toolbar">
      <button type="button" class="btn" data-action="back">← Back to editing</button>
      <div class="viewer-toggles">
        <label class="check"><input type="checkbox" id="viewer-body" checked> <span>Body</span></label>
        <label class="check"><input type="checkbox" id="viewer-lid" checked> <span>Lid</span></label>
      </div>
      <span class="spacer"></span>
      <button type="button" class="btn" data-action="viewer-export">Export</button>
      <button type="button" class="btn primary" data-action="viewer-preview">Preview again</button>
    </div>`;

  if (building) {
    root.innerHTML = `
    <section class="viewer-view">
      ${toolbar}
      <div class="panel">
        <p class="stage">Building <code>${escapeHtml(id)}</code> — ${escapeHtml(state.build.stage ?? "preparing")} · ${((state.build.elapsed_ms ?? 0) / 1000).toFixed(1)} s</p>
        <p class="hint">The viewer loads the real meshes when the build finishes.</p>
      </div>
    </section>`;
    bindActions(root);
    return;
  }

  if (state.build.state === "failed" && state.build.revision === state.revision) {
    root.innerHTML = `
    <section class="viewer-view">
      ${toolbar}
      <div class="errors"><p>${escapeHtml(state.build.error ?? "the build failed")}</p></div>
      <p class="hint">Fix the spec or the workspace and try “Preview again”.</p>
    </section>`;
    bindActions(root);
    return;
  }

  root.innerHTML = `
  <section class="viewer-view">
    ${toolbar}
    ${
      stale
        ? `<div class="stale-banner"><span>This preview is stale: the spec changed after the build it shows.</span><button type="button" class="btn small" data-action="viewer-preview">Rebuild preview</button></div>`
        : ""
    }
    <div class="viewer-stage" id="viewer-stage"></div>
    <p class="hint">Left-drag to orbit, wheel to zoom. The meshes are the real build artifacts of <code>${escapeHtml(id)}</code>; nothing is generated in the browser.</p>
  </section>`;
  bindActions(root);

  const stage = root.querySelector("#viewer-stage");
  const toggleBody = root.querySelector("#viewer-body");
  const toggleLid = root.querySelector("#viewer-lid");

  (async () => {
    try {
      const listing = await api.artifacts(id);
      if (!stage.isConnected) return;
      const names = listing.artifacts.map((artifact) => artifact.name);
      const hasLid = names.includes("lid.stl");
      toggleLid.disabled = !hasLid;
      const handle = mountViewer(stage, {
        bodyUrl: api.artifactUrl(id, "body.stl"),
        lidUrl: hasLid ? api.artifactUrl(id, "lid.stl") : null,
      });
      toggleBody.addEventListener("change", () => handle.setBodyVisible(toggleBody.checked));
      toggleLid.addEventListener("change", () => handle.setLidVisible(toggleLid.checked));
    } catch (error) {
      if (stage.isConnected) {
        stage.innerHTML = `<p class="viewer-message">${escapeHtml(error.message)}</p>`;
      }
    }
  })();
}

function bindActions(root) {
  root.querySelectorAll("[data-action]").forEach((element) => {
    const handler = actions[element.dataset.action];
    if (!handler) return;
    element.addEventListener("click", (event) => {
      event.preventDefault();
      handler(element);
    });
  });
}

// ------------------------------------------------------------ routing

function navigate(hash) {
  if (location.hash === hash) {
    route().catch(showApiError);
    return;
  }
  location.hash = hash;
}

async function route() {
  const hash = location.hash.replace(/^#\/?/, "");
  const parts = hash.split("/").filter((part) => part.length > 0);

  if (parts[0] === "standard") {
    state.view = "standard";
    render();
    await ensureStandard();
    return;
  }

  if (parts[0] === "box" && parts[1]) {
    const key = decodeURIComponent(parts[1]);
    state.view = parts[2] === "viewer" ? "viewer" : "editor";
    if (state.key !== key) {
      await openBox(key);
      return;
    }
    render();
    return;
  }

  state.view = "boxes";
  render();
  await refreshBoxes();
}

// ------------------------------------------------------------- actions

const actions = {
  render,
  navigate,
  ensureStandard,
  mutate,
  undo,
  redo,
  reset: resetToPreset,
  "start-preset": startFromPreset,
  "discard-draft": discardDraft,
  "new-box": newBoxAction,
  "refresh-boxes": () => refreshBoxes(),
  "open-box": (element) => openBoxAction(element.dataset.box),
  "duplicate-box": (element) => duplicateBoxAction(element.dataset.box),
  save: saveAction,
  "save-as": saveAsAction,
  build: buildAction,
  export: exportAction,
  publish: publishAction,
  index: indexAction,
  preview: previewAction,
  copy: copyAction,
  back: () => navigate(editorHash()),
  "viewer-export": viewerExportAction,
  "viewer-preview": previewAction,
  isDirty: dirty,
  previewStale,
  selectLeaf,
  selectDivider,
  splitLeaf,
  renameLeaf,
  mergeLeaf,
  setDividerAt,
  applyFitCandidate,
};

// ------------------------------------------------------------- startup

setEditorContext({ state, actions, preset: PRESET });
setPanelsContext({ state, actions, api, preset: PRESET });

window.addEventListener("hashchange", () => {
  route().catch(showApiError);
});

document.addEventListener("keydown", (event) => {
  const tag = event.target?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  if (!(event.ctrlKey || event.metaKey)) return;
  if (event.key === "z" && !event.shiftKey) {
    event.preventDefault();
    undo();
  } else if (event.key === "y" || (event.key === "z" && event.shiftKey)) {
    event.preventDefault();
    redo();
  }
});

async function start() {
  try {
    const health = await api.health();
    state.systemVersion = health.system_version;
  } catch {
    // The version line is cosmetic; the UI still works without it.
  }
  if (!location.hash) location.hash = "#/boxes";
  await route();
}

start().catch(showApiError);
