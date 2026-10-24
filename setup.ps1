# Set up Lattice on a Windows 10/11 x64 machine with one command.
#
#   Double-click setup.cmd, or in PowerShell:
#     powershell -ExecutionPolicy Bypass -File .\setup.ps1
#   Without a checkout (downloads the source to %USERPROFILE%\lattice first):
#     irm https://raw.githubusercontent.com/J-Reed700/lattice/main/setup.ps1 | iex
#
# The Windows counterpart of setup.sh. Each step is skipped when what it
# installs is already there:
#   1. With winget: Visual Studio 2022 Build Tools (C++ workload), Git (its
#      bash runs the sidecar fetch script), Python 3, Node.js LTS and rustup.
#   2. Rust stable when rustc is older than the build needs.
#   3. npm ci, and the pinned llama-server sidecar.
#   4. A release build with the NSIS installer, installed silently for the
#      current user, then opened.
# On first launch the app offers a chat and an embedding model; click Install.
#
# Options: -Dev (run `npm run tauri:dev` after step 3), -Deps (stop after
#          step 3), -Check (report only), -NoLaunch, -Yes (never prompt).

[CmdletBinding()]
param(
    [switch]$Dev,
    [switch]$Deps,
    [switch]$Check,
    [switch]$NoLaunch,
    [switch]$Yes
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$RepoSlug = 'J-Reed700/lattice'
# wasmtime 48 declares rust-version 1.95, the highest in Cargo.lock.
$MinRust = [version]'1.95.0'

function Write-Step([string]$Text) { Write-Host ''; Write-Host "==> $Text" -ForegroundColor Cyan }
function Write-Ok([string]$Text) { Write-Host "  ok       $Text" -ForegroundColor Green }
function Write-Missing([string]$Text) { Write-Host "  missing  $Text" -ForegroundColor Yellow }
function Stop-Setup([string]$Text) { Write-Host ''; Write-Host "error: $Text" -ForegroundColor Red; exit 1 }

# ---------------------------------------------------------------------------
# No checkout: fetch the source, then run the setup.ps1 inside it.
# ---------------------------------------------------------------------------
$Root = $PSScriptRoot
if (-not $Root -or -not (Test-Path (Join-Path $Root 'crates\lattice-desktop\tauri.conf.json'))) {
    $dest = if ($env:LATTICE_DIR) { $env:LATTICE_DIR } else { Join-Path $env:USERPROFILE 'lattice' }
    $ref = if ($env:LATTICE_REF) { $env:LATTICE_REF } else { 'main' }
    if (-not (Test-Path (Join-Path $dest 'crates\lattice-desktop\tauri.conf.json'))) {
        if ((Test-Path $dest) -and (Get-ChildItem $dest -Force | Select-Object -First 1)) {
            Stop-Setup "$dest exists and is not a Lattice checkout. Set LATTICE_DIR to another folder."
        }
        Write-Step "Downloading the Lattice source ($ref) to $dest"
        $zip = Join-Path $env:TEMP "lattice-$ref.zip"
        Invoke-WebRequest "https://codeload.github.com/$RepoSlug/zip/$ref" -OutFile $zip -UseBasicParsing
        $unpack = Join-Path $env:TEMP "lattice-unpack-$([guid]::NewGuid())"
        Expand-Archive $zip -DestinationPath $unpack
        Move-Item (Get-ChildItem $unpack -Directory | Select-Object -First 1).FullName $dest
        Remove-Item $zip, $unpack -Recurse -Force
    }
    $forward = @()
    if ($Dev) { $forward += '-Dev' }
    if ($Deps) { $forward += '-Deps' }
    if ($Check) { $forward += '-Check' }
    if ($NoLaunch) { $forward += '-NoLaunch' }
    if ($Yes) { $forward += '-Yes' }
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $dest 'setup.ps1') @forward
    exit $LASTEXITCODE
}
Set-Location $Root

if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -ne 'AMD64') {
    Stop-Setup "Lattice builds on x64 Windows only (the pinned llama-server has no ARM64 build)."
}

$Mode = 'install'
if ($Dev) { $Mode = 'dev' }
if ($Deps) { $Mode = 'deps' }
if ($Check) { $Mode = 'check' }
$script:NeedsWork = $false

function Confirm-Step([string]$Question) {
    if ($Yes) { return $true }
    $answer = Read-Host "  $Question [Y/n]"
    return (-not $answer) -or ($answer -match '^[Yy]')
}

function Update-SessionPath {
    $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
    $user = [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:Path = "$machine;$user"
    $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
    if ((Test-Path $cargoBin) -and ($env:Path -notlike "*$cargoBin*")) { $env:Path = "$cargoBin;$env:Path" }
}

function Invoke-Native([string]$What, [scriptblock]$Command) {
    & $Command
    if ($LASTEXITCODE -ne 0) { Stop-Setup "$What failed (exit code $LASTEXITCODE)." }
}

function Install-WithWinget([string]$Id, [string[]]$Extra = @()) {
    if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
        Stop-Setup "winget is missing. Install 'App Installer' from the Microsoft Store, then rerun setup."
    }
    $wingetArgs = @('install', '--id', $Id, '--exact', '--silent', '--accept-package-agreements', '--accept-source-agreements') + $Extra
    & winget @wingetArgs
    # -1978335189: already installed and no newer version.
    if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) {
        Stop-Setup "winget install $Id failed (exit code $LASTEXITCODE)."
    }
    Update-SessionPath
}

function Test-Command([string]$Name) { return [bool](Get-Command $Name -ErrorAction SilentlyContinue) }

# ---------------------------------------------------------------------------
# 1. Build tools
# ---------------------------------------------------------------------------
function Confirm-BuildTools {
    $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
    $vc = $null
    $ErrorActionPreference = 'Continue'
    if (Test-Path $vswhere) {
        $vc = & $vswhere -products * -latest -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    }
    if ($vc) { Write-Ok "MSVC build tools ($vc)"; return }
    Write-Missing 'Visual Studio 2022 Build Tools with the C++ workload'
    $script:NeedsWork = $true
    if ($Mode -eq 'check') { return }
    if (-not (Confirm-Step 'Install the Visual Studio Build Tools (about 6 GB)?')) { Stop-Setup 'The MSVC toolchain is required.' }
    Install-WithWinget 'Microsoft.VisualStudio.2022.BuildTools' @('--override', '--wait --quiet --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
    Write-Ok 'MSVC build tools'
}

function Confirm-Git {
    $bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
    if (Test-Path $bash) { Write-Ok "Git Bash ($bash)"; return }
    Write-Missing 'Git for Windows (its bash runs the sidecar fetch script)'
    $script:NeedsWork = $true
    if ($Mode -eq 'check') { return }
    Install-WithWinget 'Git.Git'
    Write-Ok 'Git for Windows'
}

function Get-Python {
    # Windows PowerShell turns a native command's redirected stderr into
    # errors, which 'Stop' would make fatal.
    $ErrorActionPreference = 'Continue'
    foreach ($candidate in @('py -3', 'python', 'python3')) {
        $parts = $candidate.Split(' ')
        if (-not (Test-Command $parts[0])) { continue }
        $exe = $parts[0]
        $pre = @($parts | Select-Object -Skip 1)
        # The Microsoft Store alias for python exits non-zero without a real install.
        $version = & $exe @pre -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>$null
        if ($LASTEXITCODE -eq 0 -and $version -and ([version]$version -ge [version]'3.9')) { return $version }
    }
    return $null
}

function Confirm-Python {
    $version = Get-Python
    if ($version) { Write-Ok "Python $version"; return }
    Write-Missing 'Python 3.9 or newer (verifies the sidecar binaries)'
    $script:NeedsWork = $true
    if ($Mode -eq 'check') { return }
    Install-WithWinget 'Python.Python.3.12'
    if (-not (Get-Python)) { Stop-Setup 'Python was installed but is not on PATH yet; open a new terminal and rerun setup.' }
    Write-Ok "Python $(Get-Python)"
}

# ---------------------------------------------------------------------------
# 2. Rust and Node.js
# ---------------------------------------------------------------------------
function Get-RustVersion {
    if (-not (Test-Command rustc)) { return $null }
    $line = & rustc --version
    if ($line -match '^rustc (\d+\.\d+\.\d+)') { return [version]$Matches[1] }
    return $null
}

function Confirm-Rust {
    Update-SessionPath
    $have = Get-RustVersion
    if ($have -and $have -ge $MinRust -and (Test-Command cargo)) { Write-Ok "Rust $have"; return }
    if ($have) { Write-Missing "Rust $MinRust or newer (found $have)" } else { Write-Missing 'Rust' }
    $script:NeedsWork = $true
    if ($Mode -eq 'check') { return }
    if (-not (Test-Command rustup)) { Install-WithWinget 'Rustlang.Rustup' }
    Invoke-Native 'rustup' { rustup toolchain install stable --profile minimal }
    $env:RUSTUP_TOOLCHAIN = 'stable'
    $have = Get-RustVersion
    if (-not $have -or $have -lt $MinRust) { Stop-Setup "rustc $have is still older than $MinRust." }
    Write-Ok "Rust $have"
}

# Same rule as setup.sh: package.json "engines.node" when present, otherwise
# the README's "Node.js 20.19+ or 22.12+".
$NodeCheck = @'
const fs = require("fs");
let range = ">=20.19.0 <21 || >=22.12.0";
try { range = JSON.parse(fs.readFileSync(process.argv[1], "utf8")).engines.node || range; } catch {}
const parse = (s) => s.split(".").map(Number).concat([0, 0, 0]).slice(0, 3);
const cmp = (a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2];
const have = parse(process.versions.node);
const ok = range.split("||").some((alt) => alt.trim().split(/\s+/).every((term) => {
  const m = /^(>=|<=|>|<|=)?v?([\d.]+)$/.exec(term);
  if (!m) return false;
  const c = cmp(have, parse(m[2]));
  return { ">=": c >= 0, "<=": c <= 0, ">": c > 0, "<": c < 0 }[m[1]] ?? c === 0;
}));
console.log(range);
process.exit(ok ? 0 : 1);
'@

function Test-Node {
    if (-not (Test-Command node)) { return $false }
    $ErrorActionPreference = 'Continue'
    # A file, not `node -e`: Windows PowerShell drops the quotes inside
    # arguments it passes to native programs.
    $checker = Join-Path $env:TEMP 'lattice-node-check.js'
    Set-Content -Path $checker -Value $NodeCheck -Encoding ASCII
    $script:NodeRange = & node $checker (Join-Path $Root 'package.json') 2>$null
    return ($LASTEXITCODE -eq 0)
}

function Confirm-Node {
    if (Test-Node) { Write-Ok "Node.js $(& node --version)"; return }
    if (Test-Command node) { Write-Missing "Node.js $script:NodeRange (found $(& node --version))" } else { Write-Missing 'Node.js' }
    $script:NeedsWork = $true
    if ($Mode -eq 'check') { return }
    Install-WithWinget 'OpenJS.NodeJS.LTS'
    if (-not (Test-Node)) { Stop-Setup "The installed Node.js does not satisfy $script:NodeRange; install a matching version and rerun." }
    Write-Ok "Node.js $(& node --version)"
}

# ---------------------------------------------------------------------------
# 3. Project dependencies
# ---------------------------------------------------------------------------
function Confirm-NpmPackages {
    $stamp = Join-Path $Root 'node_modules\.lattice-setup-lock'
    $digest = (Get-FileHash (Join-Path $Root 'package-lock.json') -Algorithm SHA256).Hash.ToLower()
    if ((Test-Path $stamp) -and ((Get-Content $stamp -Raw).Trim() -eq $digest)) { Write-Ok 'npm packages match package-lock.json'; return }
    Write-Missing 'npm packages'
    $script:NeedsWork = $true
    if ($Mode -eq 'check') { return }
    Invoke-Native 'npm ci' { npm ci --no-audit --no-fund }
    Set-Content -Path $stamp -Value $digest -NoNewline
    Write-Ok 'npm packages installed'
}

function Confirm-Sidecar {
    $bash = Join-Path $env:ProgramFiles 'Git\bin\bash.exe'
    $fetch = 'crates/lattice-desktop/scripts/fetch-llama-binaries.sh'
    if ($Mode -eq 'check') {
        $ErrorActionPreference = 'Continue'
        $sidecarOk = $false
        if (Test-Path $bash) {
            & $bash $fetch --check *> $null
            $sidecarOk = ($LASTEXITCODE -eq 0)
        }
        if ($sidecarOk) {
            Write-Ok 'llama-server sidecar matches the lock'
        } else {
            Write-Missing "llama-server sidecar (bash $fetch)"
            $script:NeedsWork = $true
        }
        return
    }
    Invoke-Native 'Fetching the llama-server sidecar' { & $bash $fetch }
    Write-Ok 'llama-server sidecar'
}

# ---------------------------------------------------------------------------
# 4. Build, install, open
# ---------------------------------------------------------------------------
function Get-TargetDir {
    Push-Location (Join-Path $Root 'crates\lattice-desktop')
    try { return ((& cargo metadata --format-version 1 --no-deps) | ConvertFrom-Json).target_directory }
    finally { Pop-Location }
}

function Install-App {
    Write-Step 'Building Lattice (release, NSIS installer). The first build takes a while.'
    Invoke-Native 'The release build' { npm run tauri -- build --bundles nsis }
    $bundle = Join-Path (Get-TargetDir) 'release\bundle\nsis'
    $installer = Get-ChildItem $bundle -Filter '*-setup.exe' | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if (-not $installer) { Stop-Setup "The build produced no installer in $bundle" }
    Get-Process -Name 'lattice-desktop' -ErrorAction SilentlyContinue | Stop-Process -Force
    Write-Step "Installing $($installer.Name)"
    $process = Start-Process $installer.FullName -ArgumentList '/S' -Wait -PassThru
    if ($process.ExitCode -ne 0) { Stop-Setup "The installer exited with code $($process.ExitCode)." }
    $exe = $null
    foreach ($dir in @((Join-Path $env:LOCALAPPDATA 'Lattice'), (Join-Path $env:LOCALAPPDATA 'Programs\Lattice'), (Join-Path $env:ProgramFiles 'Lattice'))) {
        if (-not (Test-Path $dir)) { continue }
        $exe = Get-ChildItem $dir -Filter '*.exe' | Where-Object { $_.Name -notmatch '^(llama-server|uninstall)' } | Select-Object -First 1
        if ($exe) { break }
    }
    if (-not $exe) { Stop-Setup 'The installer finished but the Lattice executable was not found.' }
    Write-Ok "Installed $($exe.FullName)"
    if (-not $NoLaunch) {
        Start-Process $exe.FullName
        Write-Ok 'Lattice is starting'
    }
}

# ---------------------------------------------------------------------------
# Run
# ---------------------------------------------------------------------------
switch ($Mode) {
    'check' { Write-Step 'Checking what this machine has (nothing will be installed)' }
    { $_ -in 'dev', 'deps' } { Write-Step "Installing what Lattice needs to build in $Root" }
    default { Write-Step "Setting up Lattice from $Root" }
}
Confirm-BuildTools
Confirm-Git
Confirm-Python
Confirm-Rust
Confirm-Node
if ($Mode -eq 'check' -and -not (Test-Command npm)) {
    Write-Missing 'npm packages (needs Node.js first)'
    Write-Missing 'llama-server sidecar'
    $script:NeedsWork = $true
} else {
    Confirm-NpmPackages
    Confirm-Sidecar
}

if ($Mode -eq 'check') {
    if ($script:NeedsWork) { Write-Host ''; Write-Host 'Run setup.ps1 to install what is missing.'; exit 1 }
    Write-Host ''; Write-Host 'Everything is in place. setup.ps1 builds and installs the app.'
    exit 0
}
if ($Mode -eq 'deps') {
    Write-Host ''; Write-Host 'Everything the build needs is installed. Next: setup.ps1 (install the app) or setup.ps1 -Dev.'
    exit 0
}
if ($Mode -eq 'dev') {
    Write-Step 'Starting the app in development mode (npm run tauri:dev)'
    npm run tauri:dev
    exit $LASTEXITCODE
}
Install-App
Write-Host ''
Write-Host 'Lattice is installed.' -ForegroundColor Green
Write-Host 'On first launch it shows "Set up AI" with a chat model and an embedding model'
Write-Host 'picked for this machine. Click Install; the downloads continue in the'
Write-Host 'background and both models are switched on when they finish.'
