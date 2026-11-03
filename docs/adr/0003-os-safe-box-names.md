# Box names are OS-safe file names, not a slug format

**Status:** accepted, 2026-11-03. Replaces the generator's two-form box-id rule (lowercase kebab-case or `<N>x<M>-<height>U-<slug>-<rev>`).

The spec `id` is not just a label: it is the registry folder (`boxes/<id>/`), the build directory (`build/<id>/`), and the export base (`exports/<id>/`). The old validator therefore enforced a naming convention that the filesystem never asked for — uppercase, spaces, underscores, and dots were rejected in the UI with no functional reason, which made the editor fight the user over a file name.

**Decision:** a box id is any one OS-safe file name. The validator accepts everything a normal file name may contain — spaces, uppercase, dots, underscores, parentheses, and non-ASCII letters — and rejects only names that would break path handling or the file system:

- empty, `.`, `..`
- `/` or `\` (path separators)
- control characters (NUL, newlines, and the rest)
- the Windows-reserved characters `< > : " | ? *`
- a trailing space or dot (Windows silently drops them)
- Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `CONIN$`, `CONOUT$`, `CLOCK$`, `COM1`-`COM9`, `LPT1`-`LPT9`, with or without an extension)
- more than 255 bytes

The historical kebab-case and standard `<N>x<M>-<height>U-<slug>-<rev>` forms remain valid; they are simply no longer the only forms. The rule lives in `spec::is_valid_id`, so the CLI (`validate`, `build`, `--publish`, `index`) and the UI's `save` accept exactly the same names; the UI uses the typed name as the folder, no sanitising and no second derivation. The SYS-2.0 standard's §10 naming convention stays a recommendation for box folders, not a generator constraint.

**Consequences:** `--publish` and the UI's publish keep the one target rule — the folder must be named after the spec `id`, now with more legal names. A box whose folder name differs from its id still opens (folder scan) but still cannot publish. Names remain portable: the Windows-only restrictions are rejected so a registry synced or zipped to another OS does not break.

**Rejected:** sanitising the typed name into a slug (the folder would differ from what the user typed and from the id); keeping the old rule for the CLI while loosening only the UI (two rules for the same identifier); accepting every byte the Linux VFS allows (breaks portability and makes the same registry invalid on Windows).
