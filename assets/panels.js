// The editor's panels: the spec form, the fit assistant, the build/export
// results, the fixed print recipe, and the read-only System standard view.
//
// The panels never decide validity: the form writes the design and the Rust
// validator answers through `/api/plan` (ADR-0002). The one piece of local
// arithmetic is the fit-assistant display, which only renders what
// `/api/fit` returned.

let ctx = null;

/** Wires the panels to the app store and actions. */
export function setPanelsContext(context) {
  ctx = context;
}

// ---------------------------------------------------------------- helpers

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** `80.0` — millimetres with at most one decimal, trimmed. */
function mm(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) return "—";
  const rounded = Math.round(value * 10) / 10;
  return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
}

/** `0.93` — hours with two decimals. */
function hours(value) {
  return typeof value === "number" ? value.toFixed(2) : "—";
}

function grams(value) {
  return typeof value === "number" ? `${value.toFixed(1)} g` : "—";
}

/** One `<tr><th>label</th><td class="value">value</td></tr>` table. */
function table(rows) {
  const body = rows
    .map(
      ([label, value]) =>
        `<tr><th>${escapeHtml(label)}</th><td class="value">${escapeHtml(value)}</td></tr>`,
    )
    .join("");
  return `<table class="values"><tbody>${body}</tbody></table>`;
}

/** The deviations from the calibrated preset, as one line; empty when none. */
export function deviationLine(design) {
  if (!design) return "";
  const preset = ctx.preset;
  const parts = [];
  if (
    design.footprint[0] !== preset.footprint[0] ||
    design.footprint[1] !== preset.footprint[1]
  ) {
    parts.push(
      `footprint ${design.footprint[0]} × ${design.footprint[1]} (preset ${preset.footprint[0]} × ${preset.footprint[1]})`,
    );
  }
  if (design.height_u !== preset.height_u) {
    parts.push(`height ${design.height_u}U (preset ${preset.height_u}U)`);
  }
  if (!design.lid) {
    parts.push("lid off (preset on)");
  } else if (!design.lid_ribs) {
    parts.push("ribs off (preset on)");
  }
  return parts.join(" · ");
}

/** The validity/deviation/dirty badges of the editor toolbar. */
export function badgesHtml() {
  const { design, plan, planError } = ctx.state;
  const badges = [];
  if (planError) {
    badges.push(`<span class="badge invalid">invalid</span>`);
  } else if (plan && !plan.valid) {
    badges.push(`<span class="badge invalid">invalid</span>`);
  } else if (plan) {
    badges.push(`<span class="badge ok">valid</span>`);
  } else {
    badges.push(`<span class="badge">checking…</span>`);
  }

  if (design) {
    const deviation = deviationLine(design);
    badges.push(
      deviation
        ? `<span class="badge custom" title="${escapeHtml(deviation)}">custom: ${escapeHtml(deviation)}</span>`
        : `<span class="badge ok">calibrated preset: 4 × 4 · 6U · lid · ribs</span>`,
    );
    if (ctx.actions.isDirty()) {
      badges.push(`<span class="badge dirty">unsaved changes</span>`);
    }
    if (ctx.state.restoredDraft) {
      badges.push(`<span class="badge dirty">restored draft</span>`);
    }
    if (ctx.actions.previewStale()) {
      badges.push(
        `<button type="button" class="badge custom" data-action="preview" title="The 3D preview shows an older spec revision">3D preview stale — rebuild</button>`,
      );
    }
  }
  return badges.join("");
}

// ------------------------------------------------------------- spec form

/** The design form: the whole variable surface of one spec. */
export function designFormHtml() {
  const { design, plan, standard } = ctx.state;
  if (!design) return "";
  const body = plan?.body;

  const footprintOptions = (selected) => {
    const { min_footprint_units, max_footprint_units } = standard.limits;
    const units = [];
    for (let value = min_footprint_units; value <= max_footprint_units; value += 1) {
      units.push(value);
    }
    // An invalid spec may carry an out-of-range value; show it rather than
    // silently displaying a different footprint.
    if (!units.includes(selected)) units.push(selected);
    return units
      .map(
        (value) =>
          `<option value="${value}" ${value === selected ? "selected" : ""}>${value}</option>`,
      )
      .join("");
  };

  const heightOptions = () => {
    const units = [...standard.limits.heights_u];
    if (!units.includes(design.height_u)) units.push(design.height_u);
    return units
      .map(
        (value) =>
          `<option value="${value}" ${value === design.height_u ? "selected" : ""}>${value}U</option>`,
      )
      .join("");
  };

  const contents = design.contents ?? {};

  return `
  <section class="panel">
    <h2>Spec</h2>
    <div class="stack">
      <div>
        <span class="field-label">Box id</span>
        <code>${escapeHtml(design.id)}</code>
      </div>
      <div class="grid-2">
        <label>
          <span>Footprint N × M</span>
          <span class="grid-2">
            <select data-field="footprint-n">${footprintOptions(design.footprint[0])}</select>
            <select data-field="footprint-m">${footprintOptions(design.footprint[1])}</select>
          </span>
        </label>
        <label>
          <span>Height</span>
          <select data-field="height_u">${heightOptions()}</select>
        </label>
      </div>
      ${body ? `<p class="readout">Outer ${mm(body.outer_width_mm)} × ${mm(body.outer_depth_mm)} × ${mm(body.total_height_mm)} mm · interior ${mm(body.interior_width_mm)} × ${mm(body.interior_depth_mm)} × ${mm(body.interior_height_mm)} mm</p>` : ""}
      ${plan ? estimateHtml(plan.estimate) : ""}
      <div class="grid-2">
        <label class="check"><input type="checkbox" data-field="lid" ${design.lid ? "checked" : ""}> <span>Lid</span></label>
        <label class="check"><input type="checkbox" data-field="lid_ribs" ${design.lid_ribs ? "checked" : ""} ${design.lid ? "" : "disabled"}> <span>Friction ribs</span></label>
      </div>
      <label>
        <span>Notes (for <code>notes.md</code>)</span>
        <textarea data-field="notes" rows="3">${escapeHtml(design.notes ?? "")}</textarea>
      </label>
    </div>
  </section>

  <section class="panel">
    <h2>Contents</h2>
    <div class="stack">
      <label>
        <span>Description</span>
        <input type="text" data-field="contents-description" value="${escapeHtml(contents.description ?? "")}">
      </label>
      <div class="grid-3">
        <label><span>Length (mm)</span><input type="number" min="0" step="1" data-field="contents-max_length_mm" value="${contents.max_length_mm ?? ""}"></label>
        <label><span>Width (mm)</span><input type="number" min="0" step="1" data-field="contents-max_width_mm" value="${contents.max_width_mm ?? ""}"></label>
        <label><span>Height (mm)</span><input type="number" min="0" step="1" data-field="contents-max_height_mm" value="${contents.max_height_mm ?? ""}"></label>
      </div>
      <div class="grid-2">
        <label><span>Weight (kg)</span><input type="number" min="0" step="0.1" data-field="contents-weight_kg" value="${contents.weight_kg ?? ""}"></label>
        <label class="check"><input type="checkbox" data-field="contents-soft" ${contents.soft ? "checked" : ""}> <span>Soft item or cable</span></label>
      </div>
      ${contentsWarningsHtml()}
    </div>
  </section>`;
}

/** The live `contents.*` warnings from the last plan. */
function contentsWarningsHtml() {
  const warnings = (ctx.state.plan?.warnings ?? []).filter((warning) =>
    warning.startsWith("contents."),
  );
  if (warnings.length === 0) return "";
  return `<div class="warnings"><p>Contents fit</p><ul>${warnings
    .map((warning) => `<li>${escapeHtml(warning)}</li>`)
    .join("")}</ul></div>`;
}

/** The plan's mass and print-time estimate, exactly as `/api/plan` returned
 * it (the same estimate the build summary reports). */
function estimateHtml(estimate) {
  if (!estimate) return "";
  return `<div class="estimate"><span>Estimate</span><span>${grams(
    estimate.mass_g,
  )}</span><span>${hours(estimate.print_hours)} h print</span></div>`;
}

/** Binds every form field to one design mutation. */
export function bindDesignForm(scope) {
  scope.querySelectorAll("[data-field]").forEach((element) => {
    element.addEventListener("change", () => {
      const field = element.dataset.field;
      ctx.actions.mutate((design) => {
        switch (field) {
          case "footprint-n":
            design.footprint = [Number(element.value), design.footprint[1]];
            break;
          case "footprint-m":
            design.footprint = [design.footprint[0], Number(element.value)];
            break;
          case "height_u":
            design.height_u = Number(element.value);
            break;
          case "lid":
            design.lid = element.checked;
            if (!element.checked) design.lid_ribs = false;
            break;
          case "lid_ribs":
            design.lid_ribs = element.checked;
            break;
          case "notes": {
            const notes = element.value.trim();
            design.notes = notes.length > 0 ? element.value : null;
            break;
          }
          default: {
            if (!field.startsWith("contents-")) break;
            const key = field.slice("contents-".length);
            const contents = design.contents ?? {
              description: null,
              soft: false,
              max_length_mm: null,
              max_width_mm: null,
              max_height_mm: null,
              weight_kg: null,
            };
            if (key === "description") {
              const text = element.value.trim();
              contents.description = text.length > 0 ? element.value : null;
            } else if (key === "soft") {
              contents.soft = element.checked;
            } else {
              const parsed = Number(element.value);
              contents[key] =
                element.value === "" || !Number.isFinite(parsed) ? null : parsed;
            }
            design.contents = isEmptyContents(contents) ? null : contents;
          }
        }
      });
    });
  });
}

function isEmptyContents(contents) {
  return (
    (contents.description ?? null) === null &&
    contents.soft !== true &&
    (contents.max_length_mm ?? null) === null &&
    (contents.max_width_mm ?? null) === null &&
    (contents.max_height_mm ?? null) === null &&
    (contents.weight_kg ?? null) === null
  );
}

// ----------------------------------------------------------- fit assistant

/** The fit assistant: item dimensions to viable footprint/height combos. */
export function fitAssistantHtml() {
  return `
  <details class="panel" data-key="fit">
    <summary>Fit assistant</summary>
    <p class="hint">Item bounding box and weight; the standard's §8 clearances are applied by the server.</p>
    <div class="stack">
      <div class="grid-3">
        <label><span>Length (mm)</span><input type="number" min="1" step="1" id="fit-length" value="200"></label>
        <label><span>Width (mm)</span><input type="number" min="1" step="1" id="fit-width" value="120"></label>
        <label><span>Height (mm)</span><input type="number" min="1" step="1" id="fit-height" value="60"></label>
      </div>
      <div class="grid-2">
        <label><span>Weight (kg, optional)</span><input type="number" min="0" step="0.1" id="fit-weight"></label>
        <label class="check"><input type="checkbox" id="fit-soft"> <span>Soft item or cable</span></label>
      </div>
      <label><span>Description (recorded with the contents)</span><input type="text" id="fit-description" placeholder="USB cables"></label>
      <div>
        <button class="btn" id="fit-check" type="button">Check fit</button>
      </div>
      <div class="fit-output" id="fit-results"></div>
    </div>
  </details>`;
}

/** Binds the fit form; results are rendered from `/api/fit` only. */
export function bindFitAssistant(scope) {
  const button = scope.querySelector("#fit-check");
  if (!button) return;
  const results = scope.querySelector("#fit-results");

  button.addEventListener("click", async () => {
    const item = {
      length_mm: Number(scope.querySelector("#fit-length").value),
      width_mm: Number(scope.querySelector("#fit-width").value),
      height_mm: Number(scope.querySelector("#fit-height").value),
      weight_kg:
        scope.querySelector("#fit-weight").value === ""
          ? null
          : Number(scope.querySelector("#fit-weight").value),
      soft: scope.querySelector("#fit-soft").checked,
    };
    item.description = scope.querySelector("#fit-description").value.trim() || null;

    results.innerHTML = `<p class="hint">checking…</p>`;
    try {
      const fit = await ctx.api.fit({
        length_mm: item.length_mm,
        width_mm: item.width_mm,
        height_mm: item.height_mm,
        weight_kg: item.weight_kg,
        soft: item.soft,
      });
      results.innerHTML = fitResultsHtml(fit, item);
    } catch (error) {
      results.innerHTML = `<p class="error">${escapeHtml(error.message)}</p>`;
    }
  });

  results.addEventListener("click", (event) => {
    const candidate = event.target.closest(".fit-candidate");
    if (!candidate) return;
    const item = {
      description: scope.querySelector("#fit-description").value.trim() || null,
      soft: scope.querySelector("#fit-soft").checked,
      length_mm: Number(scope.querySelector("#fit-length").value),
      width_mm: Number(scope.querySelector("#fit-width").value),
      height_mm: Number(scope.querySelector("#fit-height").value),
      weight_kg:
        scope.querySelector("#fit-weight").value === ""
          ? null
          : Number(scope.querySelector("#fit-weight").value),
    };
    ctx.actions.applyFitCandidate(
      {
        footprint: [
          Number(candidate.dataset.n),
          Number(candidate.dataset.m),
        ],
        height_u: Number(candidate.dataset.height),
      },
      item,
    );
  });
}

function fitResultsHtml(fit, item) {
  const { design } = ctx.state;
  const current =
    design &&
    fit.viable.some(
      (candidate) =>
        candidate.footprint[0] === design.footprint[0] &&
        candidate.footprint[1] === design.footprint[1] &&
        candidate.height_u === design.height_u,
    );
  const weight = fit.weight_ok
    ? ""
    : `<p class="warnings">The item is above the 10 kg per-body limit; the weight must come down.</p>`;
  if (fit.viable.length === 0) {
    return `${weight}<p class="hint">No footprint up to 4 × 4 and height up to 12U fits this item with the ${
      fit.soft ? "soft" : "rigid"
    } clearances (${mm(fit.clearances.per_side_mm)} mm per side, ${mm(
      fit.clearances.height_clearance_mm,
    )} mm above).</p>`;
  }
  const candidates = fit.viable
    .map((candidate) => {
      const isCurrent =
        design &&
        candidate.footprint[0] === design.footprint[0] &&
        candidate.footprint[1] === design.footprint[1] &&
        candidate.height_u === design.height_u;
      return `<button type="button" class="fit-candidate${isCurrent ? " current" : ""}" data-n="${candidate.footprint[0]}" data-m="${candidate.footprint[1]}" data-height="${candidate.height_u}">${candidate.footprint[0]} × ${candidate.footprint[1]} · ${candidate.height_u}U</button>`;
    })
    .join("");
  return `
    ${weight}
    <p class="hint">${fit.soft ? "Soft" : "Rigid"} clearances: ${mm(
      fit.clearances.per_side_mm,
    )} mm per side, ${mm(fit.clearances.height_clearance_mm)} mm above, ${mm(
      fit.clearances.diagonal_margin_mm,
    )} mm diagonal margin. One click applies the combination and records the contents; a smaller footprint may not fit the current division tree, which the plan then shows.</p>
    <div class="fit-results">${candidates}</div>
    ${current ? `<p class="hint">The current spec's combination is highlighted.</p>` : ""}`;
}

// ------------------------------------------------------- build and files

/** The one build job: stage progress, summary, or failure. */
export function jobPanelHtml() {
  const { build } = ctx.state;
  if (build.state === "idle") return "";
  if (build.state === "running") {
    const stage = build.stage ?? "preparing";
    const seconds = ((build.elapsed_ms ?? 0) / 1000).toFixed(1);
    return `<section class="panel"><h2>Build</h2>
      <div class="job"><p class="stage">Building <code>${escapeHtml(build.id ?? "")}</code> — ${escapeHtml(stage)} · ${seconds} s</p>
      <p class="hint">One build at a time; the browser polls the job status.</p></div></section>`;
  }
  if (build.state === "failed") {
    return `<section class="panel"><h2>Build failed</h2>
      <div class="errors"><p>${escapeHtml(build.error ?? "the build failed")}</p></div>
      ${build.diagnostic ? `<pre class="diagnostic">${escapeHtml(build.diagnostic)}</pre>` : ""}</section>`;
  }
  const outcome = build.outcome;
  const report = outcome.report;
  const warnings = report.warnings ?? [];
  return `<section class="panel"><h2>Build</h2>
    <div class="job">
      <p class="readout">${mm(report.dimensions.outer_x_mm)} × ${mm(report.dimensions.outer_y_mm)} × ${mm(report.dimensions.height_mm)} mm · mass ${grams(report.mass_g)} · print ${hours(report.print_time_h)} h</p>
      <p class="readout">floor fillet ${mm(report.body.floor_fillet_mm)} mm${report.lid ? ` · lid roundover ${mm(report.lid.roundover_mm)} mm` : ""}</p>
      ${warnings.length > 0 ? `<div class="warnings"><p>Warnings</p><ul>${warnings.map((warning) => `<li>${escapeHtml(warning)}</li>`).join("")}</ul></div>` : `<p class="hint">No warnings.</p>`}
      <ul class="artifacts">${outcome.artifacts
        .map(
          (artifact) =>
            `<li class="artifact"><code>${escapeHtml(artifact.path)}</code><button type="button" class="btn small" data-action="copy" data-copy="${escapeHtml(artifact.path)}">copy</button></li>`,
        )
        .join("")}</ul>
      <div class="toolbar-actions">
        <button type="button" class="btn" data-action="preview">Preview in 3D</button>
        <button type="button" class="btn" data-action="export">Export</button>
      </div>
    </div></section>`;
}

/** The export result: the named copies with their host paths. */
export function exportPanelHtml() {
  const result = ctx.state.exportResult;
  if (!result) return "";
  return `<section class="panel"><h2>Export</h2>
    <p class="readout">${escapeHtml(result.out_dir)}</p>
    <ul class="artifacts">${result.artifacts
      .map(
        (artifact) =>
          `<li class="artifact"><code>${escapeHtml(artifact.path)}</code><button type="button" class="btn small" data-action="copy" data-copy="${escapeHtml(artifact.path)}">copy</button></li>`,
      )
      .join("")}</ul></section>`;
}

/** The publish result. */
export function publishPanelHtml() {
  const result = ctx.state.publishResult;
  if (!result) return "";
  return `<section class="panel"><h2>Published</h2>
    <p class="readout"><code>${escapeHtml(result.published_dir)}</code></p>
    <p class="hint">The build's <code>box.step</code>, <code>body.stl</code>${ctx.state.design?.lid ? ", <code>lid.stl</code>," : ""} and <code>notes.md</code> are in the box folder; an existing <code>notes.md</code> is never overwritten.</p></section>`;
}

/** The regenerated index result. */
export function indexPanelHtml() {
  const result = ctx.state.indexResult;
  if (!result) return "";
  return `<section class="panel"><h2>Registry index</h2>
    <p class="readout"><code>${escapeHtml(result.path)}</code> — ${result.boxes} box${result.boxes === 1 ? "" : "es"}</p></section>`;
}

/** The fixed print recipe, as a static reminder (standard §5). */
export function recipeHtml() {
  return `
  <details class="panel" data-key="recipe">
    <summary>Print recipe (fixed)</summary>
    <p class="hint">Part of the design, not a per-print choice; the UI cannot change it.</p>
    ${table([
      ["Material", "Bambu Lab PETG Basic"],
      ["Nozzle", "0.4 mm hardened steel"],
      ["Layer height", "0.20 mm"],
      ["Wall loops", "5 (2.0 mm)"],
      ["Top / bottom shells", "5 / 5 (1.0 mm)"],
      ["Infill", "40% gyroid"],
      ["Supports", "None"],
      ["Brim", "5 mm"],
      ["Build plate", "Textured PEI"],
      ["Body orientation", "Base on the plate, opening up"],
      ["Lid orientation", "Top surface on the plate, skirt up"],
      ["Temperature", "Bambu PETG Basic profile (typically 250–260 °C nozzle, 70 °C bed)"],
      ["Chamber heating", "Off"],
      ["Drying", "65 °C for 8 h if the spool was exposed to >50% RH for >72 h"],
      ["Removal", "Cool the plate, then remove the brim and deburr"],
    ])}
  </details>`;
}

// ------------------------------------------------------- system standard

/** The read-only System standard view, from `GET /api/standard`. */
export function renderStandardView(root) {
  const { standard, standardError } = ctx.state;
  if (standardError) {
    root.innerHTML = `<div class="errors"><p>${escapeHtml(standardError.message)}</p></div>`;
    return;
  }
  if (!standard) {
    root.innerHTML = `<p class="hint">loading the system standard…</p>`;
    return;
  }

  const tops = standard.dividers.tops_mm
    .map((top) => `${top.height_u}U: ${mm(top.divider_height_mm)} mm above the floor, top ${mm(top.top_mm)} mm`)
    .join(" · ");

  root.innerHTML = `
  <section class="standard-view">
    <article class="panel">
      <h2>System standard</h2>
      ${table([
        ["System version", standard.system_version],
        ["Calibration", `${standard.calibration.date} — ${standard.calibration.result}`],
        ["Coupon", standard.calibration.coupon_id],
        ["Calibration material", standard.calibration.material],
        ["Calibration nozzle", standard.calibration.nozzle],
      ])}
      <p class="hint">Read-only: the fixed architecture is anchored on the 2026-10-18 coupon calibration. The editor cannot change these values.</p>
    </article>

    <article class="panel">
      <h2>Walls and base</h2>
      ${table([
        ["Wall thickness", `${mm(standard.walls.wall_mm)} mm`],
        ["Base zone (solid)", `${mm(standard.walls.base_zone_mm)} mm`],
        ["Floor slab", `${mm(standard.walls.floor_mm)} mm`],
        ["Outer corner radius", `${mm(standard.walls.outer_radius_mm)} mm`],
        ["Inner corner radius", `${mm(standard.walls.inner_radius_mm)} mm`],
        ["Floor fillet", `${mm(standard.walls.floor_fillet_mm)} mm`],
      ])}
    </article>

    <article class="panel">
      <h2>Dividers</h2>
      ${table([
        ["Thickness", `${mm(standard.dividers.thickness_mm)} mm`],
        ["Top chamfer", `${mm(standard.dividers.top_chamfer_mm)} mm`],
        ["Junction cove", `${mm(standard.dividers.cove_mm)} mm`],
        ["Smallest compartment", `${standard.dividers.min_leaf_mm} mm`],
        ["Warning below", `${standard.dividers.warn_leaf_mm} mm`],
        ["Maximum splits per path", String(standard.dividers.max_depth)],
        ["Divider tops", tops],
      ])}
    </article>

    <article class="panel">
      <h2>Lid</h2>
      ${table([
        ["Plate", `${mm(standard.lid.plate_mm)} mm`],
        ["Skirt", `${mm(standard.lid.skirt_mm)} mm`],
        ["Clearance", `${mm(standard.lid.clearance_per_side_mm)} mm per side`],
        ["Corner radius", `${mm(standard.lid.corner_radius_mm)} mm`],
        ["Top roundover", `${mm(standard.lid.top_roundover_mm)} mm`],
        ["Bottom chamfer", `${mm(standard.lid.bottom_chamfer_mm)} mm`],
        ["Finger hole", `${mm(standard.lid.finger_hole_diameter_mm)} mm, ${mm(standard.lid.finger_hole_edge_distance_mm)} mm from the +y edge`],
        ["Friction ribs", `${mm(standard.lid.rib_interference_mm)} mm interference`],
      ])}
    </article>

    <article class="panel">
      <h2>Gridfinity interface</h2>
      ${table([
        ["Grid pitch", `${mm(standard.gridfinity.grid_mm)} mm`],
        ["Height unit", `${mm(standard.gridfinity.height_unit_mm)} mm`],
        ["Clearance", `${mm(standard.gridfinity.clearance_per_side_mm)} mm per side`],
        ["1U nominal width", `${mm(standard.gridfinity.nominal_width_1u_mm)} mm`],
        ["Base height", `${mm(standard.gridfinity.base_height_mm)} mm`],
        ["Lip height", `${mm(standard.gridfinity.lip_height_mm)} mm`],
      ])}
    </article>

    <article class="panel">
      <h2>Variable-surface limits</h2>
      ${table([
        ["Footprint", `${standard.limits.min_footprint_units} × 1 to ${standard.limits.max_footprint_units} × ${standard.limits.max_footprint_units}`],
        ["Heights", standard.limits.heights_u.map((units) => `${units}U`).join(" / ")],
        ["Default height", `${standard.limits.default_height_u}U`],
        ["Printer guard", `${mm(standard.limits.max_outer_mm)} mm`],
        ["Contents weight", `${mm(standard.limits.max_contents_kg)} kg per body`],
        ["Mass warning", `${mm(standard.limits.max_mass_warning_g)} g`],
        ["Print-time warning", `${mm(standard.limits.max_print_hours)} h`],
      ])}
    </article>

    <article class="panel">
      <h2>Contents clearances</h2>
      ${table([
        ["Rigid", `${mm(standard.contents.rigid.per_side_mm)} mm per side, ${mm(standard.contents.rigid.above_mm)} mm above`],
        ["Soft / cables", `${mm(standard.contents.soft.per_side_mm)} mm per side, ${mm(standard.contents.soft.above_mm)} mm above`],
        ["Diagonal margin", `${mm(standard.contents.diagonal_margin_mm)} mm`],
        ["Finger access", "10 mm where fingers must reach (human rule, no field)"],
      ])}
    </article>

    <article class="panel">
      <h2>Material and estimates</h2>
      ${table([
        ["Material", standard.material.name],
        ["Density", `${mm(standard.material.density_g_per_cm3)} g/cm³`],
        ["Effective flow", `${mm(standard.material.effective_flow_mm3_per_s)} mm³/s`],
      ])}
    </article>
  </section>`;
}
