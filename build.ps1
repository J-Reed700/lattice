<#
.SYNOPSIS
    Advanced Build Script for Lattice Desktop (Windows)

.DESCRIPTION
    Comprehensive build script with modular options for Windows platform.
    Supports different build modes, feature flags, and automated workflows.

.PARAMETER Mode
    Build mode: dev, release, or production (default: release)

.PARAMETER Clean
    Clean build artifacts before building

.PARAMETER SkipTests
    Skip running tests

.PARAMETER NoBundle
    Skip creating installers (faster builds)

.PARAMETER Features
    Comma-separated list of features to enable

.PARAMETER Target
    Specific Rust target triple (default: x86_64-pc-windows-msvc)

.PARAMETER Verbose
    Enable verbose logging

.PARAMETER Checksums
    Generate SHA256 checksums for build artifacts

.PARAMETER Parallel
    Enable parallel builds (default: true)

.EXAMPLE
    .\build.ps1
    Simple release build

.EXAMPLE
    .\build.ps1 -Mode dev -NoBundle
    Development build without creating installer

.EXAMPLE
    .\build.ps1 -Clean -Features "embeddings,file_watcher" -Verbose
    Clean build with specific features and verbose output

.EXAMPLE
    .\build.ps1 -Mode release -Checksums
    Release build with checksum generation
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory=$false)]
    [ValidateSet('dev', 'release', 'production')]
    [string]$Mode = 'release',

    [Parameter(Mandatory=$false)]
    [switch]$Clean,

    [Parameter(Mandatory=$false)]
    [switch]$SkipTests,

    [Parameter(Mandatory=$false)]
    [switch]$NoBundle,

    [Parameter(Mandatory=$false)]
    [string]$Features = '',

    [Parameter(Mandatory=$false)]
    [string]$Target = 'x86_64-pc-windows-msvc',

    [Parameter(Mandatory=$false)]
    [switch]$Verbose,

    [Parameter(Mandatory=$false)]
    [switch]$Checksums,

    [Parameter(Mandatory=$false)]
    [switch]$Parallel = $true,

    [Parameter(Mandatory=$false)]
    [string]$ConfigFile = 'build.config.json'
)

# ============================================================================
# Configuration
# ============================================================================

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$StartTime = Get-Date

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$TauriDir = Join-Path $ScriptDir 'src-tauri'
$LogDir = Join-Path $ScriptDir 'build-logs'
$DistDir = Join-Path $ScriptDir 'dist'

# Create log directory
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$LogFile = Join-Path $LogDir "build-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"

# ============================================================================
# Logging Functions
# ============================================================================

function Write-Log {
    param(
        [string]$Message,
        [ValidateSet('Info', 'Success', 'Warning', 'Error', 'Header')]
        [string]$Level = 'Info'
    )

    $Timestamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    $LogMessage = "[$Timestamp] [$Level] $Message"

    # Write to file
    Add-Content -Path $LogFile -Value $LogMessage

    # Write to console with colors
    switch ($Level) {
        'Info'    { Write-Host "ℹ  $Message" -ForegroundColor Cyan }
        'Success' { Write-Host "✓ $Message" -ForegroundColor Green }
        'Warning' { Write-Host "⚠  $Message" -ForegroundColor Yellow }
        'Error'   { Write-Host "✗ $Message" -ForegroundColor Red }
        'Header'  { 
            Write-Host "`n============================================================" -ForegroundColor Magenta
            Write-Host $Message -ForegroundColor White
            Write-Host "============================================================`n" -ForegroundColor Magenta
        }
    }
}

function Invoke-Command-Safe {
    param(
        [string]$Command,
        [string]$WorkingDirectory = $ScriptDir,
        [switch]$ShowOutput
    )

    if ($Verbose -or $ShowOutput) {
        Write-Log "Executing: $Command" -Level Info
    }

    try {
        if ($ShowOutput -or $Verbose) {
            $result = Invoke-Expression "& $Command 2>&1"
            if ($LASTEXITCODE -ne 0) {
                throw "Command failed with exit code $LASTEXITCODE"
            }
            return $result
        } else {
            Push-Location $WorkingDirectory
            $result = Invoke-Expression "& $Command 2>&1" | Out-String
            Pop-Location
            if ($LASTEXITCODE -ne 0) {
                throw "Command failed with exit code $LASTEXITCODE : $result"
            }
            return $result
        }
    } catch {
        Write-Log "Command failed: $_" -Level Error
        throw
    }
}

# ============================================================================
# Configuration Loading
# ============================================================================

function Get-BuildConfig {
    $ConfigPath = Join-Path $ScriptDir $ConfigFile
    
    if (Test-Path $ConfigPath) {
        try {
            $config = Get-Content $ConfigPath -Raw | ConvertFrom-Json
            Write-Log "Loaded configuration from $ConfigFile" -Level Success
            return $config
        } catch {
            Write-Log "Failed to parse config file: $_" -Level Warning
        }
    } else {
        Write-Log "Config file not found: $ConfigFile" -Level Warning
    }

    # Return default config
    return @{
        features = @{}
        build = @{
            parallel = $true
            optimization = 'release'
        }
    }
}

# ============================================================================
# Build Steps
# ============================================================================

function Test-Prerequisites {
    Write-Log "Checking Prerequisites" -Level Header

    $tools = @(
        @{Name = 'Node.js'; Command = 'node --version'},
        @{Name = 'npm'; Command = 'npm --version'},
        @{Name = 'Rust'; Command = 'rustc --version'},
        @{Name = 'Cargo'; Command = 'cargo --version'}
    )

    foreach ($tool in $tools) {
        try {
            $version = Invoke-Expression $tool.Command 2>&1 | Out-String
            Write-Log "$($tool.Name): $($version.Trim())" -Level Success
        } catch {
            Write-Log "$($tool.Name): NOT FOUND" -Level Error
            throw "$($tool.Name) is required but not installed"
        }
    }

    # Check for Visual Studio Build Tools
    $vsWhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    if (Test-Path $vsWhere) {
        Write-Log "Visual Studio Build Tools: Found" -Level Success
    } else {
        Write-Log "Visual Studio Build Tools: Not found (may cause build issues)" -Level Warning
    }
}

function Invoke-Clean {
    if (-not $Clean) { return }

    Write-Log "Cleaning Build Artifacts" -Level Header

    $pathsToClean = @(
        (Join-Path $TauriDir 'target'),
        $DistDir,
        $LogDir,
        (Join-Path $ScriptDir '.build-cache'),
        (Join-Path $ScriptDir 'node_modules' '.vite')
    )

    foreach ($path in $pathsToClean) {
        if (Test-Path $path) {
            Write-Log "Removing $path..." -Level Info
            Remove-Item -Recurse -Force $path -ErrorAction SilentlyContinue
            Write-Log "Cleaned $(Split-Path $path -Leaf)" -Level Success
        }
    }
}

function Install-Dependencies {
    Write-Log "Installing Dependencies" -Level Header

    # Install Node dependencies
    Write-Log "Installing Node.js dependencies..." -Level Info
    Invoke-Command-Safe "npm install" -ShowOutput
    Write-Log "Node.js dependencies installed" -Level Success

    # Fetch Rust dependencies
    Write-Log "Fetching Rust dependencies..." -Level Info
    Push-Location $TauriDir
    Invoke-Command-Safe "cargo fetch"
    Pop-Location
    Write-Log "Rust dependencies ready" -Level Success
}

function Invoke-Tests {
    if ($SkipTests) {
        Write-Log "Skipping tests (--SkipTests flag)" -Level Warning
        return
    }

    Write-Log "Running Tests" -Level Header

    # Frontend tests
    Write-Log "Running frontend tests..." -Level Info
    try {
        Invoke-Command-Safe "npm run test -- --run"
        Write-Log "Frontend tests passed" -Level Success
    } catch {
        Write-Log "Frontend tests failed (continuing anyway)" -Level Warning
    }

    # Backend tests
    Write-Log "Running backend tests..." -Level Info
    try {
        Push-Location $TauriDir
        Invoke-Command-Safe "cargo test"
        Pop-Location
        Write-Log "Backend tests passed" -Level Success
    } catch {
        Write-Log "Backend tests failed (continuing anyway)" -Level Warning
    }
}

function Build-Application {
    Write-Log "Building Lattice Desktop Application" -Level Header

    # Prepare build flags
    $cargoFlags = @()
    
    if ($Mode -ne 'dev') {
        $cargoFlags += '--release'
    }

    if (-not $NoBundle) {
        # Build with Tauri (includes bundling)
        Write-Log "Building with Tauri bundler..." -Level Info
        
        $tauriCmd = "npm run tauri:build"
        
        if ($Mode -eq 'dev') {
            $tauriCmd += " -- --debug"
        }
        
        if ($Features) {
            $tauriCmd += " -- --features $Features"
        }
        
        if ($Target) {
            # Install target first
            Write-Log "Installing Rust target: $Target" -Level Info
            Push-Location $TauriDir
            Invoke-Command-Safe "rustup target add $Target"
            Pop-Location
            
            $tauriCmd += " -- --target $Target"
        }

        Invoke-Command-Safe $tauriCmd -ShowOutput
    } else {
        # Build without bundling (faster)
        Write-Log "Building without bundler..." -Level Info
        
        Push-Location $TauriDir
        $cargoCmd = "cargo build $($cargoFlags -join ' ')"
        
        if ($Features) {
            $cargoCmd += " --features $Features"
        }
        
        if ($Target) {
            Invoke-Command-Safe "rustup target add $Target"
            $cargoCmd += " --target $Target"
        }
        
        Invoke-Command-Safe $cargoCmd -ShowOutput
        Pop-Location
    }

    Write-Log "Build completed successfully" -Level Success
}

function New-Checksums {
    if (-not $Checksums) { return }

    Write-Log "Generating Checksums" -Level Header

    $bundleDir = Join-Path $TauriDir "target\$(if($Mode -eq 'dev'){'debug'}else{'release'})\bundle"
    
    if (-not (Test-Path $bundleDir)) {
        Write-Log "Bundle directory not found, skipping checksums" -Level Warning
        return
    }

    $checksumFile = Join-Path $DistDir 'checksums.txt'
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

    $checksums = @()

    # Find all installer files
    $installerExtensions = @('.msi', '.exe')
    $installerFiles = Get-ChildItem -Path $bundleDir -Recurse -File | 
        Where-Object { $installerExtensions -contains $_.Extension }

    foreach ($file in $installerFiles) {
        $hash = (Get-FileHash -Path $file.FullName -Algorithm SHA256).Hash
        $relativePath = $file.FullName.Replace($bundleDir, '').TrimStart('\')
        $checksums += "$hash  $relativePath"
        Write-Log "$($file.Name): $hash" -Level Info
    }

    $checksums | Out-File -FilePath $checksumFile -Encoding utf8
    Write-Log "Checksums written to $checksumFile" -Level Success
}

function Show-Summary {
    Write-Log "Build Summary" -Level Header

    $duration = (Get-Date) - $StartTime
    Write-Log "Total build time: $($duration.ToString('mm\:ss'))" -Level Success

    # Find and display output locations
    $targetDir = Join-Path $TauriDir "target\$(if($Mode -eq 'dev'){'debug'}else{'release'})"
    
    if (Test-Path $targetDir) {
        Write-Log "Build artifacts location: $targetDir" -Level Info
        
        # List bundle files
        $bundleDir = Join-Path $targetDir 'bundle'
        if (Test-Path $bundleDir) {
            Write-Log "`nGenerated installers:" -Level Info
            Get-ChildItem -Path $bundleDir -Recurse -File -Include *.msi, *.exe | 
                ForEach-Object {
                    $size = [math]::Round($_.Length / 1MB, 2)
                    Write-Log "  - $($_.Name) ($size MB)" -Level Info
                }
        }
    }

    Write-Log "`nBuild log saved to: $LogFile" -Level Info
}

# ============================================================================
# Main Build Process
# ============================================================================

function Start-Build {
    try {
        Write-Log "Lattice Desktop Build System - Windows" -Level Header
        Write-Log "Mode: $Mode" -Level Info
        Write-Log "Target: $Target" -Level Info
        Write-Log "Bundle: $(-not $NoBundle)" -Level Info
        if ($Features) {
            Write-Log "Features: $Features" -Level Info
        }

        $config = Get-BuildConfig

        Test-Prerequisites
        Invoke-Clean
        Install-Dependencies
        Invoke-Tests
        Build-Application
        New-Checksums
        Show-Summary

        Write-Log "Build Completed Successfully! 🎉" -Level Header

    } catch {
        Write-Log "Build failed: $_" -Level Error
        Write-Log "Check log file for details: $LogFile" -Level Error
        
        if ($Verbose) {
            Write-Host $_.Exception.ToString() -ForegroundColor Red
        }
        
        exit 1
    }
}

# ============================================================================
# Entry Point
# ============================================================================

Write-Host @"

╔══════════════════════════════════════════════════════════╗
║                                                          ║
║          Lattice Desktop Build System (Windows)           ║
║                                                          ║
╚══════════════════════════════════════════════════════════╝

"@ -ForegroundColor Cyan

Start-Build

Write-Host "`n✨ All done! ✨`n" -ForegroundColor Green

