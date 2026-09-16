#!/usr/bin/env pwsh
# Lattice Desktop development helper for Windows.
#
# Mirrors the subcommand interface of dev.sh (./dev.ps1 dev,
# ./dev.ps1 build-debug, etc.) but uses Windows-native equivalents for
# anything Unix-specific (kill, paths, logs).
#
# Why not just use dev.sh under Git Bash? Most subcommands work, but
# `killall`, `pkill`, `pgrep`, the macOS DB path, and `/tmp` log paths
# silently no-op on Windows. This script does the right thing on Win.
#
# Usage: .\dev.ps1 <command>
#        .\dev.ps1 help

[CmdletBinding()]
param(
    [Parameter(Position = 0)]
    [string]$Command = 'help',

    [Parameter(Position = 1)]
    [string]$Arg1 = ''
)

$ErrorActionPreference = 'Stop'

# ============================================================================
# Paths
# ============================================================================

$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$TauriDir    = Join-Path $ProjectRoot 'src-tauri'

# Tauri's Windows AppData layout: %APPDATA%\<bundleIdentifier>\
# tauri.conf.json sets identifier = "tech.lattice.app"
$AppDataDir  = Join-Path $env:APPDATA 'tech.lattice.app'
$DbPath      = Join-Path $AppDataDir 'lattice.db'

# Models cache. Lattice's FilesystemModelStorage hardcodes
# $HOME/.cache/lattice/models -- same path on Windows under Git Bash's
# $HOME mapping (== %USERPROFILE%).
$ModelsDir   = Join-Path $env:USERPROFILE '.cache\lattice\models'

# Logs go to %TEMP% on Windows (instead of /tmp on Unix).
$LogDir      = $env:TEMP

# ============================================================================
# Output helpers -- colored, mirrors dev.sh's print_* functions
# ============================================================================

function Show-Header {
    param([string]$Text)
    Write-Host '===========================================================' -ForegroundColor Cyan
    Write-Host $Text -ForegroundColor Cyan
    Write-Host '===========================================================' -ForegroundColor Cyan
}

function Show-Success { param([string]$Text) Write-Host "[ok]   $Text" -ForegroundColor Green }
function Show-Failure { param([string]$Text) Write-Host "[fail] $Text" -ForegroundColor Red }
function Show-Warn    { param([string]$Text) Write-Host "[warn] $Text" -ForegroundColor Yellow }
function Show-Info    { param([string]$Text) Write-Host "[info] $Text" -ForegroundColor Blue }

# ============================================================================
# Process / file helpers
# ============================================================================

# Stop processes by name pattern. PowerShell's Stop-Process doesn't
# accept globs the way `killall <name>` does on Unix, so we list-then-
# stop. -ErrorAction SilentlyContinue because "no matching process" is
# success for our purposes (the goal is "this process isn't running").
function Stop-ProcessesByName {
    param([string[]]$Names)
    foreach ($name in $Names) {
        $procs = Get-Process -Name $name -ErrorAction SilentlyContinue
        if ($procs) {
            $procs | Stop-Process -Force -ErrorAction SilentlyContinue
        }
    }
}

# Robust directory removal. Cargo's target/ and node_modules/ frequently
# get held open by background processes (rust-analyzer, vscode, npm).
# Retry with explicit clear-readonly + DEL fallback.
function Remove-DirRetry {
    param(
        [string]$Path,
        [int]$Attempts = 3
    )
    for ($i = 1; $i -le $Attempts; $i++) {
        if (-not (Test-Path $Path)) { return $true }
        try {
            Remove-Item -Path $Path -Recurse -Force -ErrorAction Stop
            return $true
        } catch {
            # Clear read-only bits, kill the obvious holders, retry.
            try {
                Get-ChildItem -Path $Path -Recurse -Force -ErrorAction SilentlyContinue |
                    ForEach-Object { $_.IsReadOnly = $false }
            } catch { }
            Start-Sleep -Seconds 1
        }
    }
    return -not (Test-Path $Path)
}

# ============================================================================
# Subcommand implementations
# ============================================================================

function Invoke-KillAll {
    Show-Header 'Killing all processes'

    Show-Info 'Killing Node / Vite processes...'
    Stop-ProcessesByName 'node', 'npm', 'vite'

    Show-Info 'Killing Tauri processes...'
    Stop-ProcessesByName 'lattice-desktop', 'tauri'

    Show-Info 'Killing Rust build processes...'
    Stop-ProcessesByName 'cargo', 'rustc'

    Show-Info 'Killing llama-server sidecars...'
    Stop-ProcessesByName 'llama-server-cpu-x86_64-pc-windows-msvc',
                         'llama-server-x86_64-pc-windows-msvc',
                         'llama-server'

    Start-Sleep -Seconds 1
    Show-Success 'All processes killed'
}

function Invoke-CleanBuild {
    Show-Header 'Cleaning build artifacts'

    Push-Location $ProjectRoot
    try {
        $nm = Join-Path $ProjectRoot 'node_modules'
        if (Test-Path $nm) {
            Show-Info 'Removing node_modules...'
            if (Remove-DirRetry $nm) {
                Show-Success 'node_modules removed'
            } else {
                Show-Warn "node_modules partially removed (close VS Code / rust-analyzer if it persists)"
            }
        }

        $dist = Join-Path $ProjectRoot 'dist'
        if (Test-Path $dist) {
            Show-Info 'Removing dist...'
            Remove-Item $dist -Recurse -Force
            Show-Success 'dist removed'
        }

        if (Test-Path (Join-Path $ProjectRoot 'package-lock.json')) {
            Show-Info 'Keeping package-lock.json (reproducible npm dependency resolution)'
        }
    } finally {
        Pop-Location
    }

    Push-Location $TauriDir
    try {
        $target = Join-Path $TauriDir 'target'
        if (Test-Path $target) {
            Show-Info 'Removing Cargo target directory...'
            if (Remove-DirRetry $target) {
                Show-Success 'Cargo target removed'
            } else {
                Show-Warn 'Cargo target cleanup raced; running cargo clean...'
                cargo clean *> $null
                if (Test-Path $target) {
                    Show-Warn 'Cargo target still present (likely rust-analyzer recreating it)'
                } else {
                    Show-Success 'Cargo target removed'
                }
            }
        }

        if (Test-Path (Join-Path $TauriDir 'Cargo.lock')) {
            Show-Info 'Keeping Cargo.lock (reproducible Rust dependency resolution)'
        }
    } finally {
        Pop-Location
    }

    Show-Success 'Build artifacts cleaned'
}

function Invoke-CleanDb {
    Show-Header 'Cleaning database'

    if (Test-Path $DbPath) {
        Show-Info "Removing database at $DbPath..."
        # Glob to also catch -wal and -shm sidecar files.
        Remove-Item "$DbPath*" -Force -ErrorAction SilentlyContinue
        Show-Success 'Database removed'
    } else {
        Show-Info 'Database not found (already clean)'
    }
}

function Invoke-CleanModels {
    Show-Header 'Cleaning downloaded models'

    if (Test-Path $ModelsDir) {
        Show-Info "Removing models directory at $ModelsDir..."
        Remove-Item $ModelsDir -Recurse -Force
        Show-Success 'Models directory removed'
    } else {
        Show-Info 'Models directory not found (already clean)'
    }
}

function Invoke-CleanLogs {
    Show-Header 'Cleaning logs'

    Show-Info "Removing $LogDir\tauri_*.log files..."
    Get-ChildItem -Path $LogDir -Filter 'tauri_*.log' -ErrorAction SilentlyContinue |
        Remove-Item -Force -ErrorAction SilentlyContinue
    Show-Success 'Logs cleaned'
}

function Invoke-CleanAll {
    Show-Header 'FULL CLEAN (Nuclear Option)'
    Show-Warn 'This will remove everything: builds, database, models, logs'

    Invoke-KillAll
    Invoke-CleanBuild
    Invoke-CleanDb
    Invoke-CleanModels
    Invoke-CleanLogs

    Show-Success 'Full clean complete'
}

function Ensure-NpmRuntimeDeps {
    Push-Location $ProjectRoot
    try {
        $marker = Join-Path $ProjectRoot 'node_modules\@tauri-apps\api\package.json'
        if (-not (Test-Path $marker)) {
            Show-Warn 'Missing @tauri-apps/api in node_modules. Installing npm dependencies...'
            npm install
        }
        if (-not (Test-Path $marker)) {
            Show-Failure '@tauri-apps/api is still missing after npm install.'
            Show-Info 'Run manually: npm install @tauri-apps/api@~2.9.0 @tauri-apps/cli@~2.9.0'
            exit 1
        }
    } finally {
        Pop-Location
    }
}

function Invoke-InstallDeps {
    Show-Header 'Installing dependencies'

    Push-Location $ProjectRoot
    try {
        Show-Info 'Installing npm dependencies...'
        npm install
        Show-Success 'npm dependencies installed'
    } finally {
        Pop-Location
    }

    Push-Location $TauriDir
    try {
        Show-Info 'Building Rust dependencies (this may take a while)...'
        cargo build
        Show-Success 'Rust dependencies built'
    } finally {
        Pop-Location
    }
}

function Invoke-InstallNpm {
    Show-Header 'Installing npm dependencies'
    Push-Location $ProjectRoot
    try {
        npm install
        Show-Success 'npm dependencies installed'
    } finally {
        Pop-Location
    }
}

function Invoke-InstallRust {
    Show-Header 'Building Rust dependencies'
    Push-Location $TauriDir
    try {
        cargo build
        Show-Success 'Rust dependencies built'
    } finally {
        Pop-Location
    }
}

function Invoke-RebuildNpm {
    Show-Header 'Rebuilding npm dependencies'

    Push-Location $ProjectRoot
    try {
        $nm = Join-Path $ProjectRoot 'node_modules'
        if (Test-Path $nm) {
            Show-Info 'Removing node_modules...'
            if (Remove-DirRetry $nm) {
                Show-Success 'node_modules removed'
            }
        }
        $dist = Join-Path $ProjectRoot 'dist'
        if (Test-Path $dist) {
            Show-Info 'Removing dist...'
            Remove-Item $dist -Recurse -Force
        }

        Show-Info 'Installing npm dependencies...'
        npm install
        Show-Success 'npm dependencies installed'
    } finally {
        Pop-Location
    }
}

function Invoke-RebuildRust {
    Show-Header 'Rebuilding Rust dependencies'

    Push-Location $TauriDir
    try {
        $target = Join-Path $TauriDir 'target'
        if (Test-Path $target) {
            Show-Info 'Removing Cargo target directory...'
            if (-not (Remove-DirRetry $target)) {
                cargo clean *> $null
            }
        }

        Show-Info 'Building Rust dependencies (this may take a while)...'
        cargo build
        Show-Success 'Rust dependencies built'
    } finally {
        Pop-Location
    }
}

function Invoke-BuildFrontend {
    Show-Header 'Building frontend'
    Push-Location $ProjectRoot
    try {
        Show-Info 'Running vite build...'
        npx vite build
        Show-Success 'Frontend built'
    } finally {
        Pop-Location
    }
}

function Invoke-BuildTauriDebug {
    Show-Header 'Building Tauri (Debug)'
    Push-Location $TauriDir
    try {
        Show-Info 'Building Tauri debug binary...'
        cargo build
        Show-Success 'Tauri debug built'
        Show-Info "Binary location: $TauriDir\target\debug\lattice-desktop.exe"
    } finally {
        Pop-Location
    }
}

function Invoke-BuildTauriRelease {
    Show-Header 'Building Tauri (Release)'
    Push-Location $ProjectRoot
    try {
        Show-Info 'Building Tauri release binary (this will take several minutes)...'
        npm run tauri:build
        Show-Success 'Tauri release built'
        Show-Info "Binary location: $TauriDir\target\release\lattice-desktop.exe"
    } finally {
        Pop-Location
    }
}

function Invoke-RunFrontend {
    Show-Header 'Running frontend dev server'
    Ensure-NpmRuntimeDeps
    Push-Location $ProjectRoot
    try {
        Show-Info 'Starting Vite dev server on http://localhost:5173...'
        npm run dev
    } finally {
        Pop-Location
    }
}

function Sync-SidecarsToTarget {
    # Tauri's `tauri-plugin-shell` resolves sidecars relative to the
    # running exe's directory: `<exe-dir>/<SIDECAR_BIN>.exe`. In Lattice
    # SIDECAR_BIN is `binaries/llama-server` (see tauri.conf.json
    # externalBin + SIDECAR_BIN in sidecar_manager.rs), so the plugin
    # probes `<exe-dir>/binaries/llama-server.exe` -- NO host-triple
    # suffix is added by the plugin's resolver (verified by reading
    # tauri-plugin-shell-2.3.4/src/process/mod.rs::relative_command_path).
    # In dev that resolves to target/debug/binaries/llama-server.exe.
    # `cargo run` does not create that subdirectory, so first-time spawn
    # fails with "The system cannot find the path/file specified.".
    $srcDir   = Join-Path $TauriDir 'binaries'
    $debugDir = Join-Path $TauriDir 'target/debug'
    if (-not (Test-Path $debugDir)) { return }

    $hostExe = Join-Path $srcDir 'llama-server-x86_64-pc-windows-msvc.exe'
    if (-not (Test-Path $hostExe)) {
        Show-Warn "Sidecar source missing: $hostExe - skipping sync"
        return
    }

    $destDir = Join-Path $debugDir 'binaries'
    if (-not (Test-Path $destDir)) {
        New-Item -ItemType Directory -Path $destDir -Force | Out-Null
    }
    $destExe = Join-Path $destDir 'llama-server.exe'
    $needsCopy = $true
    if (Test-Path $destExe) {
        $srcMtime  = (Get-Item $hostExe).LastWriteTime
        $destMtime = (Get-Item $destExe).LastWriteTime
        if ($destMtime -ge $srcMtime) { $needsCopy = $false }
    }
    if ($needsCopy) {
        Copy-Item -LiteralPath $hostExe -Destination $destExe -Force
        Show-Info "Synced llama-server sidecar to target/debug/binaries/llama-server.exe"
    }
}

function Invoke-RunTauriDev {
    Show-Header 'Running Tauri dev'
    Ensure-NpmRuntimeDeps
    Sync-SidecarsToTarget
    Push-Location $ProjectRoot
    try {
        Show-Info 'Starting Tauri dev mode...'
        Show-Info 'This will start both Vite and the Tauri app'
        npm run tauri:dev
    } finally {
        Pop-Location
    }
}

function Invoke-QuickStart {
    Show-Header 'Quick Start (Testing Mode)'
    Ensure-NpmRuntimeDeps
    Sync-SidecarsToTarget

    Push-Location $ProjectRoot
    try {
        $dist = Join-Path $ProjectRoot 'dist'
        if (-not (Test-Path $dist)) {
            Show-Info 'No dist folder found, building frontend...'
            npx vite build
            Show-Success 'Frontend built'
        } else {
            Show-Info 'Using existing dist folder'
        }

        Show-Info 'Launching Tauri app...'
        npm run tauri:dev
    } finally {
        Pop-Location
    }
}

function Invoke-RunTauriDevLogs {
    Show-Header 'Running Tauri dev with logs'
    Ensure-NpmRuntimeDeps
    Sync-SidecarsToTarget

    $stamp   = Get-Date -Format 'yyyyMMdd_HHmmss'
    $logFile = Join-Path $LogDir "tauri_dev_$stamp.log"

    Push-Location $ProjectRoot
    try {
        Show-Info "Starting Tauri dev mode with logging to $logFile..."
        # PowerShell equivalent of `npm run tauri:dev 2>&1 | tee log`.
        # Tee-Object echoes to stdout AND writes to the file; -Append
        # would mix runs, so default (overwrite) is right per-invocation.
        npm run tauri:dev 2>&1 | Tee-Object -FilePath $logFile
    } finally {
        Pop-Location
    }
}

function Invoke-RunTests {
    Show-Header 'Running tests'
    Push-Location $TauriDir
    try {
        Show-Info 'Running Rust tests (lib only -- full test build hits a known esaxx-rs/cxx CRT linker mismatch on Windows; audit P-issue)'
        cargo test --lib
        Show-Success 'Rust tests complete'
    } finally {
        Pop-Location
    }
}

function Invoke-Lint {
    Show-Header 'Running linters'

    Push-Location $TauriDir
    try {
        Show-Info 'Running cargo clippy...'
        cargo clippy -- -W clippy::all
    } finally {
        Pop-Location
    }

    Push-Location $ProjectRoot
    try {
        # Project uses ESLint flat config (eslint.config.js).
        if (Test-Path 'eslint.config.js') {
            Show-Info 'Running ESLint...'
            npm run lint
        }
    } finally {
        Pop-Location
    }

    Show-Success 'Linting complete'
}

function Invoke-Format {
    Show-Header 'Formatting code'

    Push-Location $TauriDir
    try {
        Show-Info 'Running cargo fmt...'
        cargo fmt
        Show-Success 'Rust code formatted'
    } finally {
        Pop-Location
    }

    Push-Location $ProjectRoot
    try {
        if (Get-Command prettier -ErrorAction SilentlyContinue) {
            Show-Info 'Running Prettier...'
            npx prettier --write 'src/**/*.{ts,tsx,js,jsx,css,json}'
            Show-Success 'Frontend code formatted'
        }
    } finally {
        Pop-Location
    }
}

function Invoke-HealthCheck {
    Show-Header 'System Health Check'

    function Show-Tool {
        param([string]$Name, [string]$VersionArg = '--version')
        $cmd = Get-Command $Name -ErrorAction SilentlyContinue
        if ($cmd) {
            $ver = & $Name $VersionArg 2>&1 | Select-Object -First 1
            Show-Success "$Name`: $ver"
        } else {
            Show-Failure "$Name not found"
        }
    }

    Show-Tool 'node'
    Show-Tool 'npm'
    Show-Tool 'rustc'
    Show-Tool 'cargo'
    Show-Tool 'gh'

    Write-Host ''
    Show-Info 'Checking for running processes...'

    $vites = Get-Process -Name 'vite' -ErrorAction SilentlyContinue
    if ($vites) {
        Show-Warn "Vite is running (PIDs: $(($vites | ForEach-Object Id) -join ','))"
    } else {
        Show-Success 'No Vite processes running'
    }

    $tauris = Get-Process -Name 'lattice-desktop' -ErrorAction SilentlyContinue
    if ($tauris) {
        Show-Warn "Tauri app is running (PIDs: $(($tauris | ForEach-Object Id) -join ','))"
    } else {
        Show-Success 'No Tauri processes running'
    }

    $sidecars = Get-Process -ErrorAction SilentlyContinue |
        Where-Object { $_.ProcessName -like 'llama-server*' }
    if ($sidecars) {
        Show-Warn "llama-server sidecars running (PIDs: $(($sidecars | ForEach-Object Id) -join ','))"
    } else {
        Show-Success 'No llama-server sidecars running'
    }

    Write-Host ''
    Show-Info 'Disk space:'
    $drive = (Get-Item $ProjectRoot).PSDrive
    $used  = [math]::Round(($drive.Used / 1GB), 1)
    $free  = [math]::Round(($drive.Free / 1GB), 1)
    Write-Host "  $($drive.Name): $free GB free, $used GB used"

    Write-Host ''
    if (Test-Path $DbPath) {
        $size = (Get-Item $DbPath).Length / 1MB
        Show-Success ("Database exists: {0:N2} MB" -f $size)
    } else {
        Show-Info 'Database not found (will be created on first run)'
    }

    if (Test-Path $ModelsDir) {
        $items = Get-ChildItem $ModelsDir -Recurse -ErrorAction SilentlyContinue
        $size  = ($items | Measure-Object -Property Length -Sum).Sum / 1GB
        $count = (Get-ChildItem $ModelsDir -Directory -ErrorAction SilentlyContinue).Count
        Show-Success ("Models directory: {0:N2} GB ({1} models)" -f $size, $count)
    } else {
        Show-Info 'Models directory not found'
    }

    Write-Host ''
    $binaries = Join-Path $TauriDir 'binaries'
    $expected = @(
        'llama-server-aarch64-apple-darwin',
        'llama-server-x86_64-pc-windows-msvc.exe',
        'llama-server-cpu-x86_64-pc-windows-msvc.exe',
        'llama-server-x86_64-unknown-linux-gnu'
    )
    $missing = $expected | Where-Object { -not (Test-Path (Join-Path $binaries $_)) }
    if ($missing.Count -eq 0) {
        Show-Success 'Sidecar binaries: all 4 present'
    } else {
        Show-Warn "Sidecar binaries missing: $($missing -join ', ')"
        Show-Info 'Fetch with: bash src-tauri/scripts/fetch-llama-binaries.sh (or gh release download)'
    }
}

function Invoke-WatchLogs {
    param([string]$Pattern = 'tauri')

    Show-Header 'Watching logs'

    $latest = Get-ChildItem -Path $LogDir -Filter "${Pattern}_*.log" -ErrorAction SilentlyContinue |
        Sort-Object LastWriteTime -Descending |
        Select-Object -First 1

    if ($latest) {
        Show-Info "Watching $($latest.FullName)"
        Show-Info 'Press Ctrl+C to stop'
        Write-Host ''
        Get-Content -Path $latest.FullName -Wait
    } else {
        Show-Failure "No log files found matching pattern: ${Pattern}_*.log"
    }
}

function Invoke-FullRebuild {
    Show-Header 'FULL REBUILD'

    Invoke-KillAll
    Invoke-CleanBuild

    Push-Location $ProjectRoot
    try {
        Show-Info 'Installing npm dependencies...'
        npm install
        Show-Success 'npm dependencies installed'

        Show-Info 'Building frontend with Vite (creating dist directory)...'
        npx vite build
        Show-Success 'Frontend built'
    } finally {
        Pop-Location
    }

    Push-Location $TauriDir
    try {
        Show-Info 'Building Rust dependencies (this may take a while)...'
        cargo build
        Show-Success 'Rust dependencies built'
    } finally {
        Pop-Location
    }

    Show-Success 'Full rebuild complete'
    Show-Info 'You can now run: .\dev.ps1 dev'
}

function Invoke-QuickRestart {
    Show-Header 'Quick Restart'
    Invoke-KillAll
    Start-Sleep -Seconds 1
    Invoke-RunTauriDev
}

function Show-Menu {
    Show-Header 'Lattice Desktop - Development Script (Windows)'
    Write-Host ''
    Write-Host 'Usage: .\dev.ps1 <command> [arg]'
    Write-Host ''
    Write-Host 'Build Commands:' -ForegroundColor Cyan
    Write-Host '  build-frontend       Build frontend only (Vite)'
    Write-Host '  build-debug          Build Tauri in debug mode'
    Write-Host '  build-release        Build Tauri in release mode'
    Write-Host '  rebuild              Full rebuild (clean + install npm + Rust)'
    Write-Host '  rebuild-npm          Rebuild npm only (fast, no Rust)'
    Write-Host '  rebuild-rust         Rebuild Rust only (slow, keeps npm)'
    Write-Host ''
    Write-Host 'Run Commands:' -ForegroundColor Cyan
    Write-Host '  dev                  Run Tauri dev mode (frontend + backend)'
    Write-Host '  dev-logs             Run Tauri dev with logging'
    Write-Host '  quick-start          Quick start for testing (builds dist if needed)'
    Write-Host '  frontend             Run frontend dev server only'
    Write-Host '  restart              Quick restart (kill + restart)'
    Write-Host ''
    Write-Host 'Clean Commands:' -ForegroundColor Cyan
    Write-Host '  clean                Clean build artifacts'
    Write-Host '  clean-db             Clean database only'
    Write-Host '  clean-models         Clean downloaded models'
    Write-Host '  clean-logs           Clean log files'
    Write-Host '  clean-all            Nuclear clean (everything)'
    Write-Host '  kill                 Kill all running processes (incl. sidecars)'
    Write-Host ''
    Write-Host 'Install Commands:' -ForegroundColor Cyan
    Write-Host '  install              Install all dependencies (npm + Rust)'
    Write-Host '  install-npm          Install npm dependencies only (fast)'
    Write-Host '  install-rust         Build Rust dependencies only (slow)'
    Write-Host ''
    Write-Host 'Maintenance Commands:' -ForegroundColor Cyan
    Write-Host '  test                 Run tests (cargo test --lib)'
    Write-Host '  lint                 Run linters'
    Write-Host '  format               Format code'
    Write-Host '  health               System health check'
    Write-Host '  logs [pattern]       Watch logs (default: tauri)'
    Write-Host ''
    Write-Host 'Examples:' -ForegroundColor Cyan
    Write-Host '  .\dev.ps1 dev                                # Start development'
    Write-Host '  .\dev.ps1 quick-start                        # Quick launch'
    Write-Host '  .\dev.ps1 clean-all; .\dev.ps1 rebuild       # Fresh start'
    Write-Host '  .\dev.ps1 health                             # Check system'
    Write-Host '  .\dev.ps1 logs tauri                         # Tail Tauri logs'
}

# ============================================================================
# Dispatch
# ============================================================================

switch ($Command.ToLower()) {
    'build-frontend' { Invoke-BuildFrontend }
    'build-debug'    { Invoke-BuildTauriDebug }
    'build-release'  { Invoke-BuildTauriRelease }
    'rebuild'        { Invoke-FullRebuild }
    'rebuild-npm'    { Invoke-RebuildNpm }
    'rebuild-rust'   { Invoke-RebuildRust }

    'dev'            { Invoke-RunTauriDev }
    'dev-logs'       { Invoke-RunTauriDevLogs }
    'quick-start'    { Invoke-QuickStart }
    'frontend'       { Invoke-RunFrontend }
    'restart'        { Invoke-QuickRestart }

    'clean'          { Invoke-CleanBuild }
    'clean-db'       { Invoke-CleanDb }
    'clean-models'   { Invoke-CleanModels }
    'clean-logs'     { Invoke-CleanLogs }
    'clean-all'      { Invoke-CleanAll }
    'kill'           { Invoke-KillAll }

    'install'        { Invoke-InstallDeps }
    'install-npm'    { Invoke-InstallNpm }
    'install-rust'   { Invoke-InstallRust }

    'test'           { Invoke-RunTests }
    'lint'           { Invoke-Lint }
    'format'         { Invoke-Format }
    'health'         { Invoke-HealthCheck }
    'logs'           { Invoke-WatchLogs -Pattern ($Arg1 -or 'tauri') }

    'help'           { Show-Menu }
    '-h'             { Show-Menu }
    '--help'         { Show-Menu }

    default {
        if ($Command) {
            Show-Failure "Unknown command: $Command"
            Write-Host ''
        }
        Show-Menu
        exit 1
    }
}
