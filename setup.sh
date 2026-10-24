#!/usr/bin/env bash
# Set up Lattice on a macOS or Linux machine with one command.
#
#   ./setup.sh                 from a checkout
#   curl -fsSL https://raw.githubusercontent.com/J-Reed700/lattice/main/setup.sh | bash
#                              without one: downloads the source to ~/lattice first
#
# Each step is skipped when what it installs is already there, so running the
# script again only rebuilds and reinstalls the app.
#
#   1. System build tools: Xcode Command Line Tools on macOS; on Linux the
#      Tauri libraries (WebKitGTK 4.1 and friends), a C/C++ toolchain, git,
#      curl, python3 and the Vulkan loader.
#   2. Rust stable (through rustup) and Node.js 24, when missing or too old.
#   3. npm ci, and the pinned llama-server sidecar.
#   4. A release build of the app, installed as a .deb (apt), an .rpm (dnf)
#      or Lattice.app in /Applications (macOS), then opened.
#
# On its first launch the app offers a chat model and an embedding model sized
# to the machine. Clicking Install downloads both and activates them.
#
# Options:
#   --dev         stop after step 3 and run the app with `npm run tauri:dev`
#   --deps        stop after step 3 (everything a build needs is installed)
#   --check       report what is installed and what is missing; change nothing
#   --no-launch   do not open the app at the end
#   --yes         never ask before installing anything (CI, containers)
#   -h, --help    show this text
#
# Environment:
#   LATTICE_DIR   where the curl mode puts the source (default: ~/lattice)
#   LATTICE_REF   branch or tag the curl mode downloads (default: main)
#
# Windows: run setup.ps1 in PowerShell instead.

set -euo pipefail

REPO_SLUG="J-Reed700/lattice"
NODE_MAJOR=24
# wasmtime 48 declares rust-version 1.95, the highest in Cargo.lock.
MIN_RUST="1.95.0"

MODE=install
LAUNCH=1
ASSUME_YES=0
for arg in "$@"; do
  case "$arg" in
    --dev) MODE=dev ;;
    --deps) MODE=deps ;;
    --check) MODE=check ;;
    --no-launch) LAUNCH=0 ;;
    --yes|-y) ASSUME_YES=1 ;;
    -h|--help)
      sed -n '2,/^# Windows/p' "${BASH_SOURCE[0]:-setup.sh}" 2>/dev/null | sed 's/^# \{0,1\}//' \
        || echo "Usage: setup.sh [--dev | --check] [--no-launch] [--yes]"
      exit 0
      ;;
    *) echo "error: unknown option '$arg' (see --help)" >&2; exit 2 ;;
  esac
done

if [[ -t 1 ]]; then
  BOLD=$'\033[1m' BLUE=$'\033[34m' GREEN=$'\033[32m' YELLOW=$'\033[33m' RED=$'\033[31m' RESET=$'\033[0m'
else
  BOLD="" BLUE="" GREEN="" YELLOW="" RED="" RESET=""
fi
step() { printf '\n%s==> %s%s\n' "$BLUE$BOLD" "$*" "$RESET"; }
ok() { printf '  %sok%s       %s\n' "$GREEN" "$RESET" "$*"; }
missing() { printf '  %smissing%s  %s\n' "$YELLOW" "$RESET" "$*"; }
note() { printf '  %s\n' "$*"; }
die() { printf '\n%serror:%s %s\n' "$RED$BOLD" "$RESET" "$*" >&2; exit 1; }

# ---------------------------------------------------------------------------
# No checkout: fetch the source, then run the setup.sh inside it.
# ---------------------------------------------------------------------------
ROOT=""
if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
fi
if [[ -z "$ROOT" || ! -f "$ROOT/crates/lattice-desktop/tauri.conf.json" ]]; then
  dest="${LATTICE_DIR:-$HOME/lattice}"
  ref="${LATTICE_REF:-main}"
  if [[ -f "$dest/crates/lattice-desktop/tauri.conf.json" ]]; then
    step "Using the Lattice source in $dest"
    if [[ -d "$dest/.git" ]] && command -v git >/dev/null 2>&1; then
      git -C "$dest" pull --ff-only || note "Could not fast-forward $dest; building it as it is."
    fi
  elif [[ -e "$dest" && -n "$(ls -A "$dest" 2>/dev/null)" ]]; then
    die "$dest exists and is not a Lattice checkout. Set LATTICE_DIR to another folder."
  else
    step "Downloading the Lattice source ($ref) to $dest"
    if command -v git >/dev/null 2>&1; then
      git clone --depth 1 --branch "$ref" "https://github.com/$REPO_SLUG.git" "$dest"
    else
      mkdir -p "$dest"
      curl -fsSL "https://codeload.github.com/$REPO_SLUG/tar.gz/$ref" | tar -xz -C "$dest" --strip-components=1
    fi
  fi
  # Piped through bash, stdin is this script; give prompts the terminal back.
  if (: </dev/tty) 2>/dev/null; then
    exec bash "$dest/setup.sh" "$@" </dev/tty
  fi
  exec bash "$dest/setup.sh" "$@"
fi
cd "$ROOT"

# ---------------------------------------------------------------------------
# Platform
# ---------------------------------------------------------------------------
OS="$(uname -s)"
ARCH="$(uname -m)"
PKG=""
case "$OS" in
  Darwin)
    PLATFORM=macos
    macos_version="$(sw_vers -productVersion)"
    # tauri.conf.json bundle.macOS.minimumSystemVersion
    if [[ "$(printf '%s\n13.3\n' "$macos_version" | sort -V | head -n1)" != "13.3" ]]; then
      die "Lattice needs macOS 13.3 or newer; this Mac runs $macos_version."
    fi
    ;;
  Linux)
    PLATFORM=linux
    # The pinned llama-server release (crates/lattice-desktop/scripts/llama-server.lock)
    # has Linux binaries for x86_64 only.
    [[ "$ARCH" == x86_64 ]] || die "Lattice builds on x86_64 Linux only; this machine is $ARCH."
    for candidate in apt-get dnf pacman; do
      if command -v "$candidate" >/dev/null 2>&1; then PKG="$candidate"; break; fi
    done
    [[ -n "$PKG" ]] || die "No supported package manager (apt-get, dnf or pacman). Install the Tauri prerequisites listed at https://v2.tauri.app/start/prerequisites/ and run ./setup.sh --dev."
    ;;
  *)
    die "Unsupported system: $OS. On Windows, run setup.ps1 in PowerShell."
    ;;
esac

SUDO=()
if [[ "$PLATFORM" == linux && $EUID -ne 0 ]]; then
  SUDO=(sudo)
fi

confirm() {
  [[ $ASSUME_YES -eq 1 ]] && return 0
  local answer
  (: </dev/tty) 2>/dev/null || die "Cannot ask for confirmation without a terminal; rerun with --yes."
  read -r -p "  $1 [Y/n] " answer </dev/tty
  [[ -z "$answer" || "$answer" =~ ^[Yy] ]]
}

as_root() {
  if [[ ${#SUDO[@]} -gt 0 ]]; then
    command -v sudo >/dev/null 2>&1 || die "This step needs root and sudo is not installed: $*"
  fi
  "${SUDO[@]+"${SUDO[@]}"}" "$@"
}

# A release build takes longer than sudo's credential cache, so keep it fresh
# until the script exits; otherwise the install step would ask again at the end.
SUDO_KEEPALIVE=""
keep_sudo() {
  [[ ${#SUDO[@]} -gt 0 && -z "$SUDO_KEEPALIVE" ]] || return 0
  command -v sudo >/dev/null 2>&1 || die "sudo is not installed; run setup.sh as root or install sudo."
  # `sudo -v` asks for a password whenever any sudoers rule for the user needs
  # one, even when passwordless sudo (or a fresh cache) would already work.
  if ! sudo -n true 2>/dev/null; then
    sudo -v || die "sudo is needed to install system packages."
  fi
  ( while kill -0 $$ 2>/dev/null; do sudo -n true 2>/dev/null; sleep 50; done ) &
  SUDO_KEEPALIVE=$!
}
stop_sudo_keepalive() {
  [[ -n "$SUDO_KEEPALIVE" ]] && kill "$SUDO_KEEPALIVE" 2>/dev/null
  SUDO_KEEPALIVE=""
}
trap stop_sudo_keepalive EXIT

version_at_least() { # version_at_least <have> <want>
  [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n1)" == "$2" ]]
}

NEEDS_WORK=0

# ---------------------------------------------------------------------------
# 1. System build tools
# ---------------------------------------------------------------------------
macos_build_tools() {
  if xcode-select -p >/dev/null 2>&1 && xcrun --find clang >/dev/null 2>&1; then
    ok "Xcode Command Line Tools ($(xcode-select -p))"
    return
  fi
  missing "Xcode Command Line Tools"
  NEEDS_WORK=1
  [[ "$MODE" == check ]] && return
  confirm "Install the Xcode Command Line Tools?" || die "The Command Line Tools are required (clang, git, codesign)."
  # The same unattended route Homebrew's installer uses: the placeholder file
  # makes softwareupdate list the tools as an available update.
  local placeholder=/tmp/.com.apple.dt.CommandLineTools.installondemand.in-progress
  touch "$placeholder"
  local label
  label="$(softwareupdate -l 2>/dev/null | grep -B 1 -E 'Command Line Tools' | awk -F'*' '/^ *\*/ {print $2}' \
    | sed -e 's/^ *Label: //' -e 's/^ *//' | sort -V | tail -n1)"
  if [[ -n "$label" ]]; then
    note "Installing $label (this takes a few minutes)"
    sudo softwareupdate -i "$label" --verbose
    sudo xcode-select --switch /Library/Developer/CommandLineTools
  fi
  rm -f "$placeholder"
  if ! xcode-select -p >/dev/null 2>&1; then
    note "Opening Apple's installer. Click Install in the window that appears; setup continues when it finishes."
    xcode-select --install >/dev/null 2>&1 || true
    until xcode-select -p >/dev/null 2>&1; do sleep 10; done
  fi
  ok "Xcode Command Line Tools ($(xcode-select -p))"
}

# Package lists follow https://v2.tauri.app/start/prerequisites/ plus what this
# repository adds: python3 (sidecar verifier), git, and the Vulkan loader that
# the GPU build of llama-server links against.
linux_packages() {
  case "$PKG" in
    apt-get)
      echo build-essential pkg-config curl wget file git ca-certificates python3 xz-utils \
        libwebkit2gtk-4.1-dev libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libvulkan1
      ;;
    dnf)
      echo gcc gcc-c++ make pkgconf-pkg-config curl wget file git python3 xz \
        webkit2gtk4.1-devel openssl-devel libxdo-devel libappindicator-gtk3-devel librsvg2-devel vulkan-loader
      ;;
    pacman)
      echo base-devel curl wget file git python xz \
        webkit2gtk-4.1 openssl xdotool appmenu-gtk-module libappindicator-gtk3 librsvg vulkan-icd-loader
      ;;
  esac
}

linux_package_installed() {
  case "$PKG" in
    apt-get) dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q 'install ok installed' ;;
    dnf) rpm -q --whatprovides "$1" >/dev/null 2>&1 ;;
    pacman) pacman -Qq "$1" >/dev/null 2>&1 || pacman -Qqg "$1" >/dev/null 2>&1 ;;
  esac
}

linux_build_tools() {
  local wanted=() absent=()
  read -r -a wanted <<<"$(linux_packages)"
  for pkg in "${wanted[@]}"; do
    linux_package_installed "$pkg" || absent+=("$pkg")
  done
  if [[ ${#absent[@]} -eq 0 ]]; then
    ok "System packages (${#wanted[@]} via $PKG)"
    return
  fi
  missing "System packages: ${absent[*]}"
  NEEDS_WORK=1
  [[ "$MODE" == check ]] && return
  confirm "Install them with $PKG (needs sudo)?" || die "These packages are required to build Lattice."
  keep_sudo
  case "$PKG" in
    apt-get)
      as_root env DEBIAN_FRONTEND=noninteractive apt-get update
      as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y "${absent[@]}"
      ;;
    dnf) as_root dnf install -y "${absent[@]}" ;;
    # Arch does not support partial upgrades, so sync and upgrade together.
    pacman) as_root pacman -Syu --needed --noconfirm "${absent[@]}" ;;
  esac
  ok "System packages installed"
}

# ---------------------------------------------------------------------------
# 2. Rust and Node.js
# ---------------------------------------------------------------------------
ensure_rust() {
  [[ -f "$HOME/.cargo/env" ]] && . "$HOME/.cargo/env"
  local have=""
  command -v rustc >/dev/null 2>&1 && have="$(rustc --version | awk '{print $2}')"
  if [[ -n "$have" ]] && version_at_least "$have" "$MIN_RUST" && command -v cargo >/dev/null 2>&1; then
    ok "Rust $have"
    return
  fi
  if [[ -n "$have" ]]; then missing "Rust $MIN_RUST or newer (found $have)"; else missing "Rust"; fi
  NEEDS_WORK=1
  [[ "$MODE" == check ]] && return
  if command -v rustup >/dev/null 2>&1; then
    # Leave the user's default toolchain alone; build with current stable.
    rustup toolchain install stable --profile minimal
    export RUSTUP_TOOLCHAIN=stable
  else
    confirm "Install Rust with rustup (into ~/.cargo and ~/.rustup)?" || die "Rust is required."
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
    . "$HOME/.cargo/env"
  fi
  have="$(rustc --version | awk '{print $2}')"
  version_at_least "$have" "$MIN_RUST" || die "rustc $have is still older than $MIN_RUST."
  ok "Rust $have"
}

# Node.js installed by this script lives here, outside any system path.
TOOLCHAIN_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/lattice/toolchain"

# package.json "engines.node" when present, otherwise what Vite 8 accepts
# (the README's "Node.js 20.19+ or 22.12+").
node_range() {
  node -e 'try { console.log(require(process.argv[1]).engines.node || "") } catch { console.log("") }' \
    "$ROOT/package.json" 2>/dev/null | grep . || echo ">=20.19.0 <21 || >=22.12.0"
}

# Reads ranges of the form ">=a.b.c <x.y.z || >=n".
node_satisfies() {
  command -v node >/dev/null 2>&1 || return 1
  node -e '
    const parse = (s) => s.split(".").map(Number).concat([0, 0, 0]).slice(0, 3);
    const cmp = (a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
    const have = parse(process.versions.node);
    const ok = process.argv[1].split("||").some((alt) => alt.trim().split(/\s+/).every((term) => {
      const m = /^(>=|<=|>|<|=)?v?([\d.]+)$/.exec(term);
      if (!m) return false;
      const c = cmp(have, parse(m[2]));
      return { ">=": c >= 0, "<=": c <= 0, ">": c > 0, "<": c < 0 }[m[1]] ?? c === 0;
    }));
    process.exit(ok ? 0 : 1);
  ' "$1" 2>/dev/null
}

ensure_node() {
  local own
  own="$(ls -d "$TOOLCHAIN_DIR"/node-v"$NODE_MAJOR".*/bin 2>/dev/null | sort -V | tail -n1 || true)"
  [[ -n "$own" ]] && export PATH="$own:$PATH"
  local range=">=20.19.0 <21 || >=22.12.0"
  command -v node >/dev/null 2>&1 && range="$(node_range)"
  if node_satisfies "$range"; then
    ok "Node.js $(node --version) ($(command -v node))"
    return
  fi
  if command -v node >/dev/null 2>&1; then
    missing "Node.js $range (found $(node --version))"
  else
    missing "Node.js $range"
  fi
  NEEDS_WORK=1
  [[ "$MODE" == check ]] && return

  local os arch ext
  case "$PLATFORM" in macos) os=darwin ;; linux) os=linux ;; esac
  case "$ARCH" in x86_64|amd64) arch=x64 ;; arm64|aarch64) arch=arm64 ;; *) die "No Node.js build for $ARCH." ;; esac
  ext=tar.gz
  local version
  version="$(curl -fsSL https://nodejs.org/dist/index.json | python3 -c '
import json, sys
major = sys.argv[1]
print(next(r["version"] for r in json.load(sys.stdin) if r["version"].startswith("v" + major + ".")))
' "$NODE_MAJOR")" || die "Could not read the Node.js release list from nodejs.org."
  local name="node-$version-$os-$arch"
  note "Installing Node.js $version into $TOOLCHAIN_DIR"
  local tmp
  tmp="$(mktemp -d)"
  curl -fsSL "https://nodejs.org/dist/$version/$name.$ext" -o "$tmp/$name.$ext"
  curl -fsSL "https://nodejs.org/dist/$version/SHASUMS256.txt" -o "$tmp/SHASUMS256.txt"
  local expected actual
  expected="$(awk -v f="$name.$ext" '$2 == f {print $1}' "$tmp/SHASUMS256.txt")"
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$tmp/$name.$ext" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "$tmp/$name.$ext" | awk '{print $1}')"
  fi
  [[ -n "$expected" && "$expected" == "$actual" ]] || die "Checksum mismatch for $name.$ext"
  mkdir -p "$TOOLCHAIN_DIR"
  rm -rf "${TOOLCHAIN_DIR:?}/$name"
  tar -xzf "$tmp/$name.$ext" -C "$TOOLCHAIN_DIR"
  rm -rf "$tmp"
  export PATH="$TOOLCHAIN_DIR/$name/bin:$PATH"
  range="$(node_range)"
  node_satisfies "$range" || die "Node.js $version does not satisfy \"$range\"."
  ok "Node.js $version"
}

# ---------------------------------------------------------------------------
# 3. Project dependencies
# ---------------------------------------------------------------------------
lock_digest() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi
}

ensure_npm_packages() {
  local stamp="node_modules/.lattice-setup-lock" digest
  digest="$(lock_digest package-lock.json)"
  if [[ -f "$stamp" && "$(cat "$stamp")" == "$digest" ]]; then
    ok "npm packages match package-lock.json"
    return
  fi
  missing "npm packages"
  NEEDS_WORK=1
  [[ "$MODE" == check ]] && return
  npm ci --no-audit --no-fund
  echo "$digest" >"$stamp"
  ok "npm packages installed"
}

ensure_sidecar() {
  local fetch="crates/lattice-desktop/scripts/fetch-llama-binaries.sh"
  if [[ "$MODE" == check ]]; then
    if bash "$fetch" --check >/dev/null 2>&1; then
      ok "llama-server sidecar matches the lock"
    else
      missing "llama-server sidecar (bash $fetch)"
      NEEDS_WORK=1
    fi
    return
  fi
  bash "$fetch"
  ok "llama-server sidecar"
}

# ---------------------------------------------------------------------------
# 4. Build, install, open
# ---------------------------------------------------------------------------
bundle_kind() {
  case "$PLATFORM:$PKG" in
    macos:*) echo app ;;
    linux:apt-get) echo deb ;;
    linux:dnf) echo rpm ;;
    *) echo "" ;;
  esac
}

cargo_target_dir() {
  (cd crates/lattice-desktop && cargo metadata --format-version 1 --no-deps \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
}

# Memory in GB: the container limit when there is one, otherwise physical RAM.
memory_gb() {
  local bytes=""
  case "$PLATFORM" in
    macos) bytes="$(sysctl -n hw.memsize)" ;;
    linux)
      bytes="$(awk '/^MemTotal:/ {print $2 * 1024}' /proc/meminfo)"
      local limit
      limit="$(cat /sys/fs/cgroup/memory.max 2>/dev/null || true)"
      [[ "$limit" =~ ^[0-9]+$ && "$limit" -lt "$bytes" ]] && bytes="$limit"
      ;;
  esac
  echo $(( ${bytes:-0} / 1073741824 ))
}

# Swap in GB (Linux; macOS grows its swap on demand).
swap_gb() {
  [[ "$PLATFORM" == linux ]] || { echo 0; return; }
  awk '/^SwapTotal:/ {print int($2 / 1048576)}' /proc/meminfo
}

# Compiling the app crate peaks near 7 GB (measured in an x86_64 container on
# Apple Silicon); the OOM killer ends a build that runs short. Say so before a
# long build, with the fix, rather than after it.
check_build_memory() {
  [[ "$PLATFORM" == linux ]] || return 0
  local mem swap
  mem="$(memory_gb)"
  swap="$(swap_gb)"
  (( mem + swap >= 10 )) && return 0
  note "${YELLOW}This machine has ${mem} GB of memory and ${swap} GB of swap. Building the app needs"
  note "about 8 GB; with less the compiler is killed partway through. To add 8 GB of swap:${RESET}"
  note "  sudo fallocate -l 8G /swapfile && sudo chmod 600 /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile"
  confirm "Build anyway?" || die "Add swap or memory, then run ./setup.sh again."
}

build_app() {
  local kind="$1"
  check_build_memory
  # rustc holds the whole 380k-line app crate in memory (about 4 GB) and each
  # parallel job adds an optimizer thread on top. In a 7 GB container two jobs
  # peaked at 6.7 GB and were OOM-killed. Keep 4 GB for the crate and allow one
  # job per further 3 GB, unless the caller chose a number.
  if [[ -z "${CARGO_BUILD_JOBS:-}" ]]; then
    local cpus jobs
    cpus="$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)"
    jobs=$(( ($(memory_gb) - 4) / 3 ))
    (( jobs < 1 )) && jobs=1
    (( jobs > cpus )) && jobs=$cpus
    export CARGO_BUILD_JOBS=$jobs
  fi
  step "Building Lattice (release, $kind, cargo jobs: $CARGO_BUILD_JOBS). The first build takes a while."
  npm run tauri -- build --bundles "$kind"
}

install_app() {
  local kind="$1" bundle_dir artifact
  bundle_dir="$(cargo_target_dir)/release/bundle/$kind"
  case "$kind" in
    deb|rpm)
      artifact="$(ls -t "$bundle_dir"/*."$kind" 2>/dev/null | head -n1)"
      [[ -n "$artifact" ]] || die "The build produced no .$kind in $bundle_dir"
      step "Installing $(basename "$artifact")"
      keep_sudo
      if [[ "$kind" == deb ]]; then
        # --reinstall: the version number does not change between local builds.
        as_root env DEBIAN_FRONTEND=noninteractive apt-get install -y --reinstall "$artifact"
        APP_BIN="$(dpkg -L "$(dpkg-deb -f "$artifact" Package)" \
          | grep -E '^/usr/bin/[^/]+$' | grep -v llama-server | head -n1 || true)"
      else
        local name
        name="$(rpm -qp --queryformat '%{NAME}' "$artifact")"
        if rpm -q "$name" >/dev/null 2>&1; then
          as_root dnf reinstall -y "$artifact"
        else
          as_root dnf install -y "$artifact"
        fi
        APP_BIN="$(rpm -ql "$name" | grep -E '^/usr/bin/[^/]+$' | grep -v llama-server | head -n1 || true)"
      fi
      [[ -n "$APP_BIN" ]] || die "$(basename "$artifact") installed, but it put no executable in /usr/bin."
      ok "Installed $APP_BIN"
      ;;
    app)
      artifact="$bundle_dir/Lattice.app"
      [[ -d "$artifact" ]] || die "The build produced no Lattice.app in $bundle_dir"
      local apps=/Applications
      [[ -w "$apps" ]] || apps="$HOME/Applications"
      mkdir -p "$apps"
      osascript -e 'tell application id "tech.lattice.app" to quit' >/dev/null 2>&1 || true
      step "Installing Lattice.app into $apps"
      rm -rf "$apps/Lattice.app"
      ditto "$artifact" "$apps/Lattice.app"
      APP_BIN="$apps/Lattice.app"
      ok "Installed $APP_BIN"
      ;;
  esac
}

launch_app() {
  [[ $LAUNCH -eq 1 ]] || return 0
  case "$PLATFORM" in
    macos) open "$APP_BIN" ;;
    linux)
      if [[ -z "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
        note "No graphical session here; start Lattice from your app menu or run $APP_BIN."
        return 0
      fi
      nohup "$APP_BIN" >/dev/null 2>&1 &
      ;;
  esac
  ok "Lattice is starting"
}

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------
case "$MODE" in
  check) step "Checking what this machine has (nothing will be installed)" ;;
  dev|deps) step "Installing what Lattice needs to build in $ROOT" ;;
  install) step "Setting up Lattice from $ROOT" ;;
esac

case "$PLATFORM" in
  macos) macos_build_tools ;;
  linux) linux_build_tools ;;
esac
ensure_rust
ensure_node
if [[ "$MODE" == check ]] && ! command -v npm >/dev/null 2>&1; then
  missing "npm packages (needs Node.js first)"
  missing "llama-server sidecar"
  NEEDS_WORK=1
else
  ensure_npm_packages
  ensure_sidecar
fi

if [[ "$MODE" == check ]]; then
  kind="$(bundle_kind)"
  if [[ -z "$kind" ]]; then
    note "No package format for $PKG: ./setup.sh --dev runs the app from source."
  fi
  if [[ $NEEDS_WORK -eq 0 ]]; then
    printf '\nEverything is in place. ./setup.sh builds and installs the app.\n'
    exit 0
  fi
  printf '\nRun ./setup.sh to install what is missing.\n'
  exit 1
fi

if [[ "$MODE" == deps ]]; then
  printf '\nEverything the build needs is installed. Next: ./setup.sh (install the app) or ./setup.sh --dev.\n'
  exit 0
fi

if [[ "$MODE" == dev ]]; then
  step "Starting the app in development mode (npm run tauri:dev)"
  stop_sudo_keepalive
  exec npm run tauri:dev
fi

kind="$(bundle_kind)"
[[ -n "$kind" ]] || die "There is no package format for $PKG. Run ./setup.sh --dev to run Lattice from source."
build_app "$kind"
install_app "$kind"
launch_app

cat <<EOF

${GREEN}${BOLD}Lattice is installed.${RESET}
On first launch it shows "Set up AI" with a chat model and an embedding model
picked for this machine. Click Install; the downloads continue in the
background and both models are switched on when they finish.
EOF
