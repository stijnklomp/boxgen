#!/usr/bin/env bash
#
# Start the boxgen local web UI (`boxgen serve`) in Docker with one command
# (v2, ADR-0002). Works without `just`; the justfile `ui` recipe wraps it.
#
# The script ensures both pinned images exist: the base FreeCAD image
# `boxgen/freecad:<pin>` from docker/freecad.Dockerfile (built only when
# missing) and `boxgen/ui:<pin>` from docker/ui.Dockerfile (`--rebuild`
# forces the UI image). The FreeCAD pin is read from the justfile, the single
# source of truth, exactly like CI.
#
# The container runs with `--network host` instead of `-p`: `boxgen serve`
# binds 127.0.0.1 inside the container by design, and a published Docker port
# forwards to the container's bridge address, where nothing listens. Sharing
# the host network namespace makes the server's 127.0.0.1 the host's loopback,
# so the UI stays reachable from this machine only. (Linux hosts; Docker
# Desktop runs containers in a VM and does not share the host network.)
#
# `BOXGEN_FREECADCMD` is forwarded into the container when the host sets it,
# so the walkthrough's failed-build tip works through this launcher. The value
# is read inside the container: a launcher script must live at a container
# path (the workspace is mounted at /app, so `/app/<script>` works).
#
# Ctrl-C stops the server. The workspace is mounted read-write: builds and
# exports land in <workdir>/build and <workdir>/exports owned by your UID.

set -euo pipefail

port=8787
workdir=$PWD
open_browser=true
rebuild=false

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
repo_root=$(cd -- "$script_dir/.." && pwd)

die() {
    printf 'boxgen-ui: %s\n' "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Usage: boxgen-ui.sh [options]

Start the boxgen local web UI in Docker.

Options:
  --port N        TCP port to bind on 127.0.0.1 (default: 8787)
  --workdir DIR   Workspace to mount at /app (default: the current directory)
  --no-open       Do not open the browser
  --rebuild       Rebuild the boxgen/ui image even when it exists
  -h, --help      Show this help

The FreeCAD pin comes from the justfile in the boxgen repository.
BOXGEN_FREECADCMD is forwarded into the container when set; point it at a
container path such as /app/failing-freecadcmd.sh.
EOF
}

while [ $# -gt 0 ]; do
    case "$1" in
        --port)
            [ $# -ge 2 ] || die "--port needs a value"
            port=$2
            shift 2
            ;;
        --workdir)
            [ $# -ge 2 ] || die "--workdir needs a value"
            workdir=$2
            shift 2
            ;;
        --no-open)
            open_browser=false
            shift
            ;;
        --rebuild)
            rebuild=true
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            printf 'boxgen-ui: unknown option: %s\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

case "$port" in
    '' | *[!0-9]*) die "invalid port: $port (expected 1-65535)" ;;
esac
if [ "$port" -lt 1 ] || [ "$port" -gt 65535 ]; then
    die "invalid port: $port (expected 1-65535)"
fi

[ -d "$workdir" ] || die "workdir does not exist: $workdir"
workdir=$(cd -- "$workdir" && pwd)

command -v docker >/dev/null 2>&1 || die "docker is required but was not found in PATH"

justfile="$repo_root/justfile"
[ -f "$justfile" ] || die "cannot find the justfile at $justfile"
freecad_version=$(sed -n 's/^freecad_version := "\([^"]*\)"$/\1/p' "$justfile")
[ -n "$freecad_version" ] || die "could not read freecad_version from $justfile"

base_image="boxgen/freecad:$freecad_version"
ui_image="boxgen/ui:$freecad_version"

if ! docker image inspect "$base_image" >/dev/null 2>&1; then
    printf 'boxgen-ui: building %s (missing)\n' "$base_image"
    docker build --build-arg "FREECAD_VERSION=$freecad_version" \
        -f "$repo_root/docker/freecad.Dockerfile" -t "$base_image" "$repo_root"
fi

if [ "$rebuild" = true ] || ! docker image inspect "$ui_image" >/dev/null 2>&1; then
    printf 'boxgen-ui: building %s\n' "$ui_image"
    docker build --build-arg "FREECAD_VERSION=$freecad_version" \
        -f "$repo_root/docker/ui.Dockerfile" -t "$ui_image" "$repo_root"
fi

if [ ! -d "$workdir/cad" ]; then
    printf 'boxgen-ui: warning: %s has no cad/ directory; builds inside the container need the FreeCAD master at /app/cad\n' \
        "$workdir" >&2
fi

url="http://127.0.0.1:$port/"

# Waits for the server to answer /api/health, then opens the URL on the host
# (the container has no browser). Every failure is ignored: the URL is already
# on stdout, so the user can open it manually. Runs in the background.
open_ui_when_ready() {
    local url="$1"
    local health_url="${url}api/health"
    local attempt=0
    if command -v curl >/dev/null 2>&1; then
        while [ "$attempt" -lt 60 ]; do
            if curl --silent --fail --max-time 2 "$health_url" >/dev/null 2>&1; then
                break
            fi
            attempt=$((attempt + 1))
            sleep 0.5
        done
    else
        sleep 2
    fi
    if command -v xdg-open >/dev/null 2>&1; then
        xdg-open "$url" >/dev/null 2>&1 || true
    elif command -v open >/dev/null 2>&1; then
        open "$url" >/dev/null 2>&1 || true
    fi
}

printf 'boxgen-ui: %s\n' "$url"
printf 'boxgen-ui: workspace %s (Ctrl-C stops the server)\n' "$workdir"

# Forward the FreeCAD launcher override when the host sets one. The `+`
# expansion keeps the empty case safe under `set -u` on older bash.
freecadcmd_env=()
if [ -n "${BOXGEN_FREECADCMD:-}" ]; then
    freecadcmd_env=(-e "BOXGEN_FREECADCMD=$BOXGEN_FREECADCMD")
    printf 'boxgen-ui: forwarding BOXGEN_FREECADCMD=%s (container path)\n' \
        "$BOXGEN_FREECADCMD"
fi

if [ "$open_browser" = true ]; then
    open_ui_when_ready "$url" &
fi

# `--init` puts tini in front of `boxgen serve`: as the container's PID 1 the
# server would ignore the default-action SIGINT/SIGTERM that `docker run`
# proxies (the kernel drops them for PID 1), so Ctrl-C would not stop it.
# tini forwards the signal and reaps the server, making Ctrl-C (and
# `docker stop`) work.
exec docker run --rm --init \
    -u "$(id -u):$(id -g)" \
    -e HOME=/tmp/boxgen-home \
    -e FREECAD_USER_HOME=/tmp/boxgen-freecad \
    -e BOXGEN_UI_HOST_WORKDIR="$workdir" \
    -e BOXGEN_EXPECT_FREECAD_VERSION="$freecad_version" \
    ${freecadcmd_env[@]+"${freecadcmd_env[@]}"} \
    --network host \
    -v "$workdir":/app \
    "$ui_image" boxgen serve --port "$port"
