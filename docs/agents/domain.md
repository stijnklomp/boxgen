# Domain Docs

How agents should consume this repository's domain documentation before exploring or changing code.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root for `boxgen` repository terms (spec compiler, master, build directory, publish, stub).
- The **SYS-1.0 spec repository's glossary** for domain terms: `../research/3d-printed-stackable-portable-storage/GLOSSARY.md`.
- **`docs/generator.md`** in the spec repository — the authoritative generator contract. Read the section you are touching (`§3` schema, `§4` division tree, `§5` CLI/exit codes, `§6` master, `§7` validation).
- **`docs/adr/`** for decisions made in this crate. The spec repository's ADRs cover the design itself.

If a file does not exist, proceed silently; do not suggest creating it upfront. The domain-modeling workflow creates glossary entries and ADRs when terms or decisions actually resolve.

## File structure

This is a single-context repository:

```
/
├── GLOSSARY.md
├── docs/
│   ├── adr/
│   │   └── 0001-cli-from-server-template.md
│   └── agents/
└── src/
```

The SYS-1.0 spec is a separate repository, not a second context inside this one. Do not copy it into `boxgen`.

## Use the glossary's vocabulary

When an issue title, test name, commit message, or design note names a domain concept, use the glossary's term. In particular: **body**, **lid**, **case**, **compartment**, **divider**, **footprint**, **height**, **division tree**, **split**, **position**, **leaf**, **spec**, **master**, **coupon**. The spec glossary lists the synonyms to avoid.

## Flag conflicts

If a change contradicts the generator contract or the frozen standard, surface it explicitly instead of silently diverging:

> _Contradicts `docs/generator.md` §4 (division tree), but worth reopening because…_

The design standard wins over the generator contract. A fixed-property change requires a system version bump and an ADR in the spec repository, not an edit here.
