#!/usr/bin/env bash
# Runs setup.sh and the app's first-run setup on clean Ubuntu 24.04 machines
# in Docker. See e2e/turnkey/README.md.
#
#   bash e2e/turnkey/run.sh                    both stages
#   bash e2e/turnkey/run.sh setup              stage 1 only
#   bash e2e/turnkey/run.sh first-run <deb>    stage 2 only, with a .deb from stage 1
#   bash e2e/turnkey/run.sh deps <image>       `setup.sh --deps` as root on another
#                                              distribution, e.g. fedora:42
#
# Stage 1 copies this working tree (tracked files plus untracked ones that are
# not ignored) into a bare container and runs `./setup.sh --yes --no-launch`.
# It passes when the app is installed and `./setup.sh --check` finds nothing
# missing. The .deb it built is copied out for stage 2.
#
# Stage 2 installs that .deb with apt into a second clean container, starts
# the app on a virtual display, clicks Install in the "Set up AI" dialog,
# waits for both models to download and switch on, then asks a question in
# chat and waits for the answer (first_run.py). Under emulation it waits for
# the model's first text instead; see TURNKEY_FULL_ANSWER.
#
# Environment:
#   TURNKEY_CACHE=1      keep the cargo registry, npm cache and target dir in
#                        Docker volumes between runs (faster, no longer a
#                        from-nothing run). `docker volume rm` the three
#                        lattice-turnkey-* volumes to drop them.
#   TURNKEY_FULL_LTO=1   build with the shipping release profile. By default
#                        the run sets codegen-units=16 and no LTO: under x86
#                        emulation on Apple Silicon one codegen unit for the
#                        app crate takes hours, and the profile is not what
#                        this test checks.
#   TURNKEY_CPUS / TURNKEY_MEMORY   container limits (default 8 / 7g)
#   TURNKEY_KEEP=1       leave the containers running afterwards for a look inside
#   TURNKEY_FULL_ANSWER=1  wait for the saved chat answer even under emulation
#
# The deps stage checks the package lists for other package managers without
# a build: `./setup.sh --yes --deps` must succeed and `./setup.sh --check`
# must then find nothing missing.
#
# Results: e2e-results/turnkey/<timestamp>/ (logs, the .deb, screenshots,
# summary.json).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
HERE="$ROOT/e2e/turnkey"
PLATFORM=linux/amd64
STAMP="$(date +%Y%m%dT%H%M%S)"
OUT="$ROOT/e2e-results/turnkey/$STAMP"
SETUP_BOX="lattice-turnkey-setup-$STAMP"
DESKTOP_BOX="lattice-turnkey-desktop-$STAMP"
DEPS_BOX="lattice-turnkey-deps-$STAMP"
CPUS="${TURNKEY_CPUS:-8}"
MEMORY="${TURNKEY_MEMORY:-7g}"

stage="${1:-all}"
case "$stage" in
  all|setup|first-run|deps) ;;
  *) echo "usage: run.sh [setup | first-run <deb> | deps <image>]" >&2; exit 2 ;;
esac

mkdir -p "$OUT"
log() { printf '\n[turnkey %s] %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$OUT/run.log"; }
fail() { log "FAIL: $*"; exit 1; }

cleanup() {
  [[ "${TURNKEY_KEEP:-0}" == 1 ]] && return
  docker rm -f "$SETUP_BOX" "$DESKTOP_BOX" "$DEPS_BOX" >/dev/null 2>&1 || true
}
trap cleanup EXIT

command -v docker >/dev/null || fail "docker is not installed"
docker info >/dev/null 2>&1 || fail "the Docker daemon is not running"

build_image() {
  log "Building the clean $1 image (ubuntu:24.04, $PLATFORM)"
  docker build --quiet --platform "$PLATFORM" --target "$1" -t "lattice-turnkey:$1" "$HERE" >/dev/null
}

pack_source() {
  log "Packing the working tree"
  (
    cd "$ROOT"
    git ls-files -z --cached --others --exclude-standard \
      | while IFS= read -r -d '' f; do [[ -e "$f" ]] && printf '%s\0' "$f"; done \
      | COPYFILE_DISABLE=1 tar --null -T - -czf "$OUT/source.tar.gz"
  )
  log "Source archive: $(du -h "$OUT/source.tar.gz" | cut -f1)"
}

# The app crate's compiler peaks near 7 GB under emulation, more than Docker
# Desktop's default VM (8 GB, 1 GB swap) has free. Check before an hour-long
# build. The fix below adds swap to the running VM until it restarts; raising
# Docker Desktop's memory setting works too.
check_vm_memory() {
  local total
  total="$(docker run --rm --platform "$PLATFORM" lattice-turnkey:fresh \
    awk '/^(MemTotal|SwapTotal):/ {kb += $2} END {print int(kb / 1048576)}' /proc/meminfo)"
  (( total >= 14 )) && return 0
  log "The Docker VM has ${total} GB of memory plus swap; building the app needs about 14 GB here."
  log "Add 8 GB of swap to the VM (lasts until Docker restarts):"
  log "  docker run --rm --privileged --pid=host -v /var/lib/lattice-turnkey-swap:/swapdir alpine sh -c 'fallocate -l 8G /swapdir/swapfile && chmod 600 /swapdir/swapfile && mkswap /swapdir/swapfile && swapon /swapdir/swapfile'"
  fail "not enough memory in the Docker VM"
}

run_setup() {
  build_image fresh
  check_vm_memory
  pack_source

  local profile=(-e CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 -e CARGO_PROFILE_RELEASE_LTO=off)
  [[ "${TURNKEY_FULL_LTO:-0}" == 1 ]] && profile=()
  local volumes=()
  if [[ "${TURNKEY_CACHE:-0}" == 1 ]]; then
    volumes=(-v lattice-turnkey-cargo:/home/ubuntu/.cargo/registry
             -v lattice-turnkey-npm:/home/ubuntu/.npm
             -v lattice-turnkey-target:/home/ubuntu/lattice/target)
  fi

  docker run -d --platform "$PLATFORM" --name "$SETUP_BOX" --cpus "$CPUS" --memory "$MEMORY" \
    ${profile[@]+"${profile[@]}"} ${volumes[@]+"${volumes[@]}"} \
    lattice-turnkey:fresh sleep infinity >/dev/null
  if [[ ${#volumes[@]} -gt 0 ]]; then
    docker exec -u root "$SETUP_BOX" chown -R ubuntu:ubuntu \
      /home/ubuntu/.cargo /home/ubuntu/.npm /home/ubuntu/lattice
  fi
  docker cp "$OUT/source.tar.gz" "$SETUP_BOX:/tmp/source.tar.gz"
  docker exec "$SETUP_BOX" bash -c 'mkdir -p ~/lattice && tar -xzf /tmp/source.tar.gz -C ~/lattice 2>/dev/null'

  log "Stage 1: ./setup.sh --yes --no-launch (log: setup.log)"
  local started=$SECONDS
  if ! docker exec "$SETUP_BOX" bash -c 'cd ~/lattice && ./setup.sh --yes --no-launch' >"$OUT/setup.log" 2>&1; then
    tail -n 40 "$OUT/setup.log"
    fail "setup.sh exited non-zero"
  fi
  log "setup.sh finished in $(( (SECONDS - started) / 60 )) min"

  local deb
  deb="$(docker exec "$SETUP_BOX" bash -c 'ls -t ~/lattice/target/release/bundle/deb/*.deb | head -n1')"
  mkdir -p "$OUT/deb"
  docker cp "$SETUP_BOX:$deb" "$OUT/deb/"
  DEB="$OUT/deb/$(basename "$deb")"

  local package
  package="$(docker exec "$SETUP_BOX" dpkg-deb -f "$deb" Package)"
  docker exec "$SETUP_BOX" dpkg-query -W -f='${Status}\n' "$package" | grep -q 'install ok installed' \
    || fail "package $package is not installed"
  docker exec "$SETUP_BOX" dpkg -L "$package" >"$OUT/installed-files.txt"
  grep -qx '/usr/bin/llama-server' "$OUT/installed-files.txt" || fail "the package installs no llama-server"
  grep -q 'learning-python/python.wasm$' "$OUT/installed-files.txt" || fail "the package installs no bundled Python"
  log "Installed: $package ($(grep -c . "$OUT/installed-files.txt") paths)"

  log "Stage 1: ./setup.sh --check must now find nothing missing"
  docker exec "$SETUP_BOX" bash -c 'cd ~/lattice && ./setup.sh --check' >"$OUT/check.log" 2>&1 \
    || { cat "$OUT/check.log"; fail "setup.sh --check still reports something missing"; }
  log "Stage 1 passed"
}

run_first_run() {
  local deb="$1"
  [[ -f "$deb" ]] || fail "no .deb at $deb"
  build_image desktop
  log "Stage 2: installing $(basename "$deb") with apt on a clean machine"
  docker run -d --platform "$PLATFORM" --name "$DESKTOP_BOX" --cpus "$CPUS" --memory "$MEMORY" \
    --shm-size 1g lattice-turnkey:desktop sleep infinity >/dev/null
  docker cp "$deb" "$DESKTOP_BOX:/tmp/lattice.deb"
  docker cp "$HERE/first_run.py" "$DESKTOP_BOX:/tmp/first_run.py"
  docker exec "$DESKTOP_BOX" bash -c \
    'sudo apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y /tmp/lattice.deb' \
    >"$OUT/apt-install.log" 2>&1 || { tail -n 30 "$OUT/apt-install.log"; fail "apt could not install the .deb"; }

  log "Stage 2: first launch, Install, downloads, one chat answer (log: first-run.log)"
  # Under emulation (an x86 image on Apple Silicon) the CPU model took 18
  # minutes to its first token, so there the run stops at the first text.
  local answer=(--answer-timeout 1800)
  if [[ "${TURNKEY_FULL_ANSWER:-0}" != 1 && "$(uname -m)" != x86_64 ]]; then
    answer+=(--first-text-only --answer-timeout 3600)
    log "Emulated x86: the chat step passes at the model's first text (TURNKEY_FULL_ANSWER=1 waits for the whole answer)"
  fi
  local status=0
  docker exec "$DESKTOP_BOX" dbus-run-session -- python3 /tmp/first_run.py --out /tmp/first-run "${answer[@]}" \
    >"$OUT/first-run.log" 2>&1 || status=$?
  docker cp "$DESKTOP_BOX:/tmp/first-run/." "$OUT/first-run/" >/dev/null 2>&1 || true
  tail -n 25 "$OUT/first-run.log"
  [[ $status -eq 0 ]] || fail "first-run check failed (screenshots in $OUT/first-run)"
  log "Stage 2 passed"
}

run_deps() {
  local image="$1"
  pack_source
  log "Deps: $image, ./setup.sh --yes --deps as root (log: deps.log)"
  docker run -d --platform "$PLATFORM" --name "$DEPS_BOX" --cpus "$CPUS" --memory "$MEMORY" \
    "$image" sleep infinity >/dev/null
  # docker cp unpacks the archive itself: minimal images may have no tar.
  local unpacked="$OUT/source"
  mkdir -p "$unpacked"
  tar -xzf "$OUT/source.tar.gz" -C "$unpacked"
  docker cp "$unpacked/." "$DEPS_BOX:/root/lattice"
  rm -rf "$unpacked"
  if ! docker exec "$DEPS_BOX" bash -c 'cd /root/lattice && ./setup.sh --yes --deps' >"$OUT/deps.log" 2>&1; then
    tail -n 30 "$OUT/deps.log"
    fail "setup.sh --deps failed on $image"
  fi
  docker exec "$DEPS_BOX" bash -c 'cd /root/lattice && ./setup.sh --check' >"$OUT/check.log" 2>&1 \
    || { cat "$OUT/check.log"; fail "setup.sh --check still reports something missing on $image"; }
  log "Deps passed on $image"
}

case "$stage" in
  all) run_setup; run_first_run "$DEB" ;;
  setup) run_setup ;;
  first-run) run_first_run "${2:?pass the .deb from a stage 1 run}" ;;
  deps) run_deps "${2:?pass a base image, e.g. fedora:42}" ;;
esac
log "All done. Results: $OUT"
