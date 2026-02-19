# Test Setup Script - Verify build environment
# Run this to check if everything is ready to build

Write-Host "`n╔══════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║                                                          ║" -ForegroundColor Cyan
Write-Host "║        Vault Desktop - Build Environment Check          ║" -ForegroundColor Cyan
Write-Host "║                                                          ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════╝`n" -ForegroundColor Cyan

$allGood = $true

# Test Node.js
Write-Host "Checking Node.js..." -ForegroundColor Yellow
try {
    $nodeVersion = node --version 2>&1
    Write-Host "  ✓ Node.js: $nodeVersion" -ForegroundColor Green
} catch {
    Write-Host "  ✗ Node.js: NOT FOUND" -ForegroundColor Red
    Write-Host "    Install: winget install OpenJS.NodeJS.LTS" -ForegroundColor Yellow
    $allGood = $false
}

# Test npm
Write-Host "`nChecking npm..." -ForegroundColor Yellow
try {
    $npmVersion = npm --version 2>&1
    Write-Host "  ✓ npm: v$npmVersion" -ForegroundColor Green
} catch {
    Write-Host "  ✗ npm: NOT FOUND" -ForegroundColor Red
    $allGood = $false
}

# Test Rust
Write-Host "`nChecking Rust..." -ForegroundColor Yellow
try {
    $rustVersion = rustc --version 2>&1
    Write-Host "  ✓ Rust: $rustVersion" -ForegroundColor Green
} catch {
    Write-Host "  ✗ Rust: NOT FOUND" -ForegroundColor Red
    Write-Host "    Install: winget install Rustlang.Rustup" -ForegroundColor Yellow
    Write-Host "    Then restart PowerShell" -ForegroundColor Yellow
    $allGood = $false
}

# Test Cargo
Write-Host "`nChecking Cargo..." -ForegroundColor Yellow
try {
    $cargoVersion = cargo --version 2>&1
    Write-Host "  ✓ Cargo: $cargoVersion" -ForegroundColor Green
} catch {
    Write-Host "  ✗ Cargo: NOT FOUND" -ForegroundColor Red
    Write-Host "    Install Rust (includes Cargo)" -ForegroundColor Yellow
    $allGood = $false
}

# Check for Visual Studio Build Tools
Write-Host "`nChecking Visual Studio Build Tools..." -ForegroundColor Yellow
$vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vsWhere) {
    $vsInstall = & $vsWhere -latest -property installationPath 2>&1
    if ($vsInstall) {
        Write-Host "  ✓ Visual Studio Build Tools: Found" -ForegroundColor Green
        Write-Host "    Location: $vsInstall" -ForegroundColor Gray
    } else {
        Write-Host "  ⚠ Visual Studio Build Tools: Installer found but no installation" -ForegroundColor Yellow
        Write-Host "    Install: winget install Microsoft.VisualStudio.2022.BuildTools" -ForegroundColor Yellow
    }
} else {
    Write-Host "  ⚠ Visual Studio Build Tools: NOT FOUND" -ForegroundColor Yellow
    Write-Host "    Install: winget install Microsoft.VisualStudio.2022.BuildTools" -ForegroundColor Yellow
    Write-Host "    Select 'Desktop development with C++' during installation" -ForegroundColor Yellow
}

# Check if build scripts exist
Write-Host "`nChecking build scripts..." -ForegroundColor Yellow
$scripts = @(
    "build.ps1",
    "build.js",
    "build.sh",
    "build-auto.js",
    "build.config.json"
)

foreach ($script in $scripts) {
    if (Test-Path $script) {
        Write-Host "  ✓ $script" -ForegroundColor Green
    } else {
        Write-Host "  ✗ $script: NOT FOUND" -ForegroundColor Red
        $allGood = $false
    }
}

# Check if package.json exists
Write-Host "`nChecking project files..." -ForegroundColor Yellow
if (Test-Path "package.json") {
    Write-Host "  ✓ package.json" -ForegroundColor Green
    
    # Check if node_modules exists
    if (Test-Path "node_modules") {
        Write-Host "  ✓ node_modules (dependencies installed)" -ForegroundColor Green
    } else {
        Write-Host "  ⚠ node_modules: NOT FOUND" -ForegroundColor Yellow
        Write-Host "    Run: npm install" -ForegroundColor Yellow
    }
} else {
    Write-Host "  ✗ package.json: NOT FOUND" -ForegroundColor Red
    $allGood = $false
}

# Check Tauri project
Write-Host "`nChecking Tauri project..." -ForegroundColor Yellow
if (Test-Path "src-tauri\Cargo.toml") {
    Write-Host "  ✓ src-tauri/Cargo.toml" -ForegroundColor Green
} else {
    Write-Host "  ✗ src-tauri/Cargo.toml: NOT FOUND" -ForegroundColor Red
    $allGood = $false
}

# Summary
Write-Host "`n" + ("=" * 60) -ForegroundColor Cyan
if ($allGood) {
    Write-Host "`n✨ Everything looks good! You're ready to build! ✨`n" -ForegroundColor Green
    Write-Host "To build the app, run one of these commands:" -ForegroundColor White
    Write-Host "  npm run build              # Simple build" -ForegroundColor Cyan
    Write-Host "  npm run build:fast         # Fast development build" -ForegroundColor Cyan
    Write-Host "  npm run build:production   # Production build" -ForegroundColor Cyan
    Write-Host "  .\build.ps1                # PowerShell script" -ForegroundColor Cyan
} else {
    Write-Host "`n⚠ Some issues found. Please install missing tools.`n" -ForegroundColor Yellow
    Write-Host "Installation commands:" -ForegroundColor White
    Write-Host "  winget install Rustlang.Rustup" -ForegroundColor Yellow
    Write-Host "  winget install OpenJS.NodeJS.LTS" -ForegroundColor Yellow
    Write-Host "  winget install Microsoft.VisualStudio.2022.BuildTools" -ForegroundColor Yellow
    Write-Host "`nAfter installation, restart PowerShell and run this script again." -ForegroundColor White
}
Write-Host "`n" + ("=" * 60) -ForegroundColor Cyan

# Show next steps
Write-Host "`nNext steps:" -ForegroundColor White
Write-Host "  1. Install any missing tools (see above)" -ForegroundColor Gray
Write-Host "  2. Run: npm install" -ForegroundColor Gray
Write-Host "  3. Run: npm run build" -ForegroundColor Gray
Write-Host "`nFor more help, see: WINDOWS_QUICKSTART.md`n" -ForegroundColor Gray

