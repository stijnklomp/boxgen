# Domain Docs

How agents should consume this repository's domain documentation before exploring or changing code.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root for `boxgen` repository terms (spec compiler, master, build directory, publish, stub).
- The **SYS-1.0 spec's glossary** for domain terms: `docs/spec/GLOSSARY.md`.
- **`docs/spec/docs/generator.md`** — the authoritative generator contract. Read the section you are touching (`§3` schema, `§4` division tree, `§5` CLI/exit codes, `§6` master, `§7` validation).
- **`docs/adr/`** for decisions made in this crate. The spec's ADRs (`docs/spec/docs/adr/`) cover the design itself.

If a file does not exist, proceed silently; do not suggest creating it upfront. The domain-modeling workflow creates glossary entries and ADRs when terms or decisions actually resolve.

## File structure

This is a single-context repository. The SYS-1.0 spec is vendored under `docs/spec/` and stays its own domain, not a second boxgen context:

```
/
├── AGENTS.md
├── GLOSSARY.md
├── docs/
│   ├── adr/          decisions for this crate
│   ├── agents/       agent workflow docs
│   └── spec/         the vendored SYS-1.0 spec (authoritative)
├── cad/              FreeCAD Python master
├── scripts/          the calibration wizard (#13)
├── src/              the Rust CLI
├── templates/        the copyable box-spec skeleton
├── tests/            CLI and integration tests
└── justfile
```

The spec was a separate local repository until 2026-10-18; it now lives at `docs/spec/` inside boxgen. Keep boxgen docs about the generator, treat the spec as authoritative for behaviour, and edit it only as a spec change (system version bump + ADR there), never as part of generator work.

## Use the glossary's vocabulary

When an issue title, test name, commit message, or design note names a domain concept, use the glossary's term. In particular: **body**, **lid**, **case**, **compartment**, **divider**, **footprint**, **height**, **division tree**, **split**, **position**, **leaf**, **spec**, **master**, **coupon**. The spec glossary lists the synonyms to avoid.

## Flag conflicts

If a change contradicts the generator contract or the frozen standard, surface it explicitly instead of silently diverging:

> _Contradicts `docs/spec/docs/generator.md` §4 (division tree), but worth reopening because…_

The design standard wins over the generator contract. A fixed-property change requires a system version bump and an ADR in the spec (`docs/spec/docs/adr/`), not an edit here.
