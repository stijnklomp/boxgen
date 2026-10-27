# boxgen

`boxgen` turns a small YAML description of a Gridfinity box into a FreeCAD model
and printable files (`box.step`, `body.stl`, `lid.stl`). You describe the
footprint, height, lid, and how the inside is divided; boxgen builds the
geometry with FreeCAD for you.

**Everything runs in Docker.** You do not need Rust, FreeCAD, or `just` on your
machine.

- [Run the web UI](#run-the-web-ui) — the easiest way in
- [Use the command line](#command-line-in-docker) — same image, no browser
- [Troubleshooting](#troubleshooting)

## Requirements

- Docker (Engine on Linux, or Docker Desktop)
- ~10 GB free disk space and an internet connection for the first run
- **Linux host recommended.** The UI only listens on `127.0.0.1` inside the
  container, so the launcher shares the host network namespace. That works on
  Linux; Docker Desktop runs containers in a VM and cannot reach the UI this
  way (see [Troubleshooting](#troubleshooting)).

## Run the web UI

From the top of this repository:

```bash
./scripts/boxgen-ui.sh
```

That is the whole setup. The first run builds two Docker images and takes a few
minutes; later runs start in seconds. When the server is ready the script opens
your browser at:

```
http://127.0.0.1:8787/
```

If the browser does not open by itself, just visit that address. Keep the
terminal open while you work — **Ctrl-C stops the UI**.

If you have `just` installed you can use the shorter `just ui` instead (it runs
the same script).

### First steps in the UI

1. **New box** creates a design; boxes already in `boxes/` are listed on the
   start page and can be opened.
2. Drag a wall of a compartment to add a divider, or click a wall without
   dragging to pick an even split from the **Add dividers** menu. Drag a
   divider to move it; click it and choose **Remove divider** to remove it.
   Everything is validated live, and the 3D plan updates as you edit.
3. **Fit assistant** suggests a footprint and height for whatever you want to
   put in the box.
4. **Save** writes `boxes/<id>/spec.yaml` (only when the design is valid).
5. **Build** runs FreeCAD: you get the model, the measured dimensions, and the
   mass/print-time estimate. **Preview in 3D** shows the real meshes,
   **Export** copies the files to `exports/`, and **Publish** copies the
   printable files into `boxes/<id>/`.

### Options

| Option | What it does |
| --- | --- |
| `--port 8799` | Serve on a different port (default `8787`). |
| `--workdir DIR` | Mount a different folder as the workspace (default: where you run it). Use the repository (or a copy of it) so builds can find `cad/`. |
| `--no-open` | Do not open the browser automatically. |
| `--rebuild` | Rebuild the UI image, for example after pulling new code. |
| `-h`, `--help` | Show the launcher's help. |

## Where your files go

The folder you start the launcher from is the workspace and is mounted into the
container, so everything is written back to your own disk:

| Path | Contents |
| --- | --- |
| `boxes/<id>/spec.yaml` | The design you saved (the source of truth). |
| `boxes/<id>/` | After **Publish**: `box.step`, `body.stl`, `lid.stl` (lid boxes), and `notes.md`. |
| `boxes/INDEX.md` | The generated registry table. |
| `build/<id>/` | Build output: `box.FCStd`, `box.step`, `body.stl`, `lid.stl`, `report.json`. |
| `exports/<name>/` | Named copies from **Export**. |

Nothing leaves your machine: the UI is local-only, with no accounts and no
cloud.

## Command line in Docker

The same image contains the `boxgen` command. After the UI image exists (run the
launcher once, or build it yourself), from the repository root:

```bash
docker run --rm -u "$(id -u):$(id -g)" \
  -e HOME=/tmp/boxgen-home \
  -e FREECAD_USER_HOME=/tmp/boxgen-freecad \
  -v "$PWD":/app -w /app \
  boxgen/ui:1.0.0 boxgen --help
```

| Command | Purpose |
| --- | --- |
| `boxgen validate <spec>` | Check a spec and print a report. |
| `boxgen build <spec> [--publish]` | Build the box; `--publish` copies the printable files into `boxes/<id>/`. |
| `boxgen divide <spec> --axis x --at 40` | Split a leaf in place, keeping the file's formatting. |
| `boxgen index` | Regenerate `boxes/INDEX.md`. |
| `boxgen coupon` | Build the calibration coupon. |

Swap `boxgen --help` in the command above for any of these, for example
`boxgen build boxes/1x1-6U-example-v1/spec.yaml`.

Building the images by hand instead of using the launcher:

```bash
docker build --build-arg FREECAD_VERSION=1.0.0 \
  -f docker/freecad.Dockerfile -t boxgen/freecad:1.0.0 .
docker build --build-arg FREECAD_VERSION=1.0.0 \
  -f docker/ui.Dockerfile -t boxgen/ui:1.0.0 .
```

## Troubleshooting

**The page does not load.** Check that the terminal still shows the server
running and open `http://127.0.0.1:8787/` yourself. If the port is already in
use, start with `--port 8799`.

**The first run seems stuck.** It is building the images (Rust binary plus
FreeCAD); the first build can take 10 minutes or more. Later starts are fast.

**Docker Desktop (macOS/Windows).** The one-command launcher relies on Linux
host networking, because the UI binds `127.0.0.1` inside the container by
design. On Docker Desktop it is not reachable from the browser; run it on a
Linux machine or a Linux VM.

**I changed the code and see no difference.** Start with `--rebuild` so the UI
image is rebuilt from the current source.

**A build fails.** The error panel shows FreeCAD's message; the same build can be
reproduced on the command line (see above) for the full log.

## More documentation

- `docs/reference.md` — the previous, full README: architecture, geometry
  rules, build pipeline internals, exit codes, and the test suite.
- `docs/spec/` — the authoritative SYS-2.0 specification (design standard,
  generator contract, calibration procedure).
- `docs/serve-walkthrough.md` — step-by-step browser walkthrough of the UI.
- `AGENTS.md` — commands, rules, and map of the codebase for contributors and AI
  agents.

## License

MIT — see `LICENSE`. The vendored Gridfinity profile module `cad/gridfinity.py`
is **LGPL-2.1-or-later** (adapted from the FreeCAD Gridfinity Workbench; see the
file header and `cad/LICENSE-LGPL-2.1`). The embedded frontend vendors three.js
0.185.0 (**MIT**) in `assets/vendor/`; source URLs and hashes are in
`assets/vendor/LICENSE-three.md`.
