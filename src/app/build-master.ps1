<#
.SYNOPSIS
    Master Build Script for Vault Complete System

.DESCRIPTION
    Builds ALL components of the Vault system:
    - Backend Server (FastAPI + PostgreSQL + Redis + ElectricSQL)
    - Python Sidecar (ML backend for Tauri app)
    - Frontend (React + Vite)
    - Tauri Desktop App (Rust + bundled Python)

.PARAMETER Components
    Comma-separated list of components to build:
    - backend: FastAPI backend server with Docker services
    - sidecar: Python ML sidecar for Tauri
    - frontend: React frontend
    - tauri: Rust Tauri backend
    - all: Build everything (default)

.PARAMETER Mode
    Build mode: dev, release, production (default: release)

.PARAMETER BackendMode
    Backend deployment mode: docker, standalone (default: docker)

.PARAMETER BundleSidecar
    Bundle Python sidecar as executable (requires PyInstaller)

.PARAMETER Clean
    Clean all build artifacts before building

.PARAMETER SkipTests
    Skip running tests

.PARAMETER ShowVerbose
    Enable verbose logging

.PARAMETER Checksums
    Generate SHA256 checksums for artifacts

.EXAMPLE
    .\build-master.ps1
    Build everything with default settings

.EXAMPLE
    .\build-master.ps1 -Components "sidecar,frontend,tauri"
    Build only desktop app components (no backend server)

.EXAMPLE
    .\build-master.ps1 -Components "all" -Clean -ShowVerbose
    Clean build of everything with verbose output

.EXAMPLE
    .\build-master.ps1 -Components "backend" -BackendMode docker
    Build only backend server with Docker

.EXAMPLE
    .\build-master.ps1 -BundleSidecar -Mode release
    Build with bundled Python sidecar executable
#>

[CmdletBinding()]
param(
    [Parameter(Mandatory=$false)]
    [string]$Components = "all",

    [Parameter(Mandatory=$false)]
    [ValidateSet('dev', 'release', 'production')]
    [string]$Mode = 'release',

    [Parameter(Mandatory=$false)]
    [ValidateSet('docker', 'standalone')]
    [string]$BackendMode = 'docker',

    [Parameter(Mandatory=$false)]
    [switch]$BundleSidecar,

    [Parameter(Mandatory=$false)]
    [switch]$Clean,

    [Parameter(Mandatory=$false)]
    [switch]$SkipTests,

    [Parameter(Mandatory=$false)]
    [switch]$ShowVerbose,

    [Parameter(Mandatory=$false)]
    [switch]$Checksums
)

$ErrorActionPreference = 'Stop'
$StartTime = Get-Date

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BackendDir = Join-Path (Split-Path -Parent $ScriptDir) 'backend'
$PythonSidecarDir = Join-Path $ScriptDir 'python_backend'
$TauriDir = Join-Path $ScriptDir 'src-tauri'
$LogDir = Join-Path $ScriptDir 'build-logs'
$DistDir = Join-Path $ScriptDir 'dist'

# Create directories
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null
$LogFile = Join-Path $LogDir "build-master-$(Get-Date -Format 'yyyyMMdd-HHmmss').log"

# ============================================================================
# Logging Functions
# ============================================================================

function Write-Log {
    param(
        [string]$Message,
        [ValidateSet('Info', 'Success', 'Warning', 'Error', 'Header', 'Step')]
        [string]$Level = 'Info'
    )

    $Timestamp = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    $LogMessage = "[$Timestamp] [$Level] $Message"
    Add-Content -Path $LogFile -Value $LogMessage

    switch ($Level) {
        'Info'    { Write-Host "  [i]  $Message" -ForegroundColor Cyan }
        'Success' { Write-Host "  [OK] $Message" -ForegroundColor Green }
        'Warning' { Write-Host "  [!]  $Message" -ForegroundColor Yellow }
        'Error'   { Write-Host "  [X]  $Message" -ForegroundColor Red }
        'Header'  {
            Write-Host "`n============================================================" -ForegroundColor Magenta
            Write-Host $Message -ForegroundColor White
            Write-Host "============================================================`n" -ForegroundColor Magenta
        }
        'Step'    { Write-Host "`n[BUILD] $Message" -ForegroundColor Yellow }
    }
}

function Invoke-CommandSafe {
    param(
        [string]$Command,
        [string]$WorkingDirectory = $ScriptDir,
        [switch]$ShowOutput,
        [string]$ErrorMessage = "Command failed"
    )

    if ($ShowVerbose -or $ShowOutput) {
        Write-Log "Executing: $Command" -Level Info
    }

    try {
        Push-Location $WorkingDirectory
        if ($ShowOutput -or $ShowVerbose) {
            $result = Invoke-Expression $Command
            if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
                throw "$ErrorMessage (exit code: $LASTEXITCODE)"
            }
        } else {
            $result = Invoke-Expression $Command | Out-String
            if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
                throw "$ErrorMessage : $result"
            }
        }
        Pop-Location
        return $result
    } catch {
        Pop-Location
        Write-Log "$ErrorMessage : $_" -Level Error
        throw
    }
}

# ============================================================================
# Component Detection
# ============================================================================

$BuildComponents = @{
    backend = $false
    sidecar = $false
    frontend = $false
    tauri = $false
}

if ($Components -eq "all") {
    $BuildComponents.sidecar = $true
    $BuildComponents.frontend = $true
    $BuildComponents.tauri = $true
    # Backend is optional for desktop-only builds
} else {
    $ComponentList = $Components -split ','
    foreach ($comp in $ComponentList) {
        $comp = $comp.Trim().ToLower()
        if ($BuildComponents.ContainsKey($comp)) {
            $BuildComponents[$comp] = $true
        }
    }
}

# ============================================================================
# Prerequisites Check
# ============================================================================

function Test-Prerequisites {
    Write-Log "Checking Prerequisites" -Level Header

    $allGood = $true

    # Node.js & npm (for frontend/Tauri)
    if ($BuildComponents.frontend -or $BuildComponents.tauri) {
        if (Get-Command node -ErrorAction SilentlyContinue) {
            $nodeVersion = node --version 2>&1
            Write-Log "Node.js: $nodeVersion" -Level Success
        } else {
            Write-Log "Node.js: NOT FOUND (required for frontend/Tauri)" -Level Error
            $allGood = $false
        }

        if (Get-Command npm -ErrorAction SilentlyContinue) {
            $npmVersion = npm --version 2>&1
            Write-Log "npm: v$npmVersion" -Level Success
        } else {
            Write-Log "npm: NOT FOUND" -Level Error
            $allGood = $false
        }
    }

    # Rust & Cargo (for Tauri)
    if ($BuildComponents.tauri) {
        if (Get-Command rustc -ErrorAction SilentlyContinue) {
            $rustVersion = rustc --version 2>&1
            Write-Log "Rust: $rustVersion" -Level Success
        } else {
            Write-Log "Rust: NOT FOUND (required for Tauri)" -Level Error
            $allGood = $false
        }

        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            $cargoVersion = cargo --version 2>&1
            Write-Log "Cargo: $cargoVersion" -Level Success
        } else {
            Write-Log "Cargo: NOT FOUND" -Level Error
            $allGood = $false
        }
    }

    # Python (for sidecar/backend)
    if ($BuildComponents.sidecar -or $BuildComponents.backend) {
        if (Get-Command python -ErrorAction SilentlyContinue) {
            $pythonVersion = python --version 2>&1
            Write-Log "Python: $pythonVersion" -Level Success
        } else {
            Write-Log "Python: NOT FOUND (required for sidecar/backend)" -Level Error
            $allGood = $false
        }
    }

    # Docker (for backend)
    if ($BuildComponents.backend -and $BackendMode -eq 'docker') {
        if (Get-Command docker -ErrorAction SilentlyContinue) {
            $dockerVersion = docker --version 2>&1
            Write-Log "Docker: $dockerVersion" -Level Success
            
            # Check Docker Compose
            $composeCheck = docker compose version 2>&1
            if ($LASTEXITCODE -eq 0) {
                Write-Log "Docker Compose: $composeCheck" -Level Success
            } else {
                Write-Log "Docker Compose: NOT FOUND" -Level Error
                $allGood = $false
            }
        } else {
            Write-Log "Docker: NOT FOUND (required for backend with Docker mode)" -Level Error
            $allGood = $false
        }
    }

    # Poetry (for backend standalone)
    if ($BuildComponents.backend -and $BackendMode -eq 'standalone') {
        if (Get-Command poetry -ErrorAction SilentlyContinue) {
            $poetryVersion = poetry --version 2>&1
            Write-Log "Poetry: $poetryVersion" -Level Success
        } else {
            Write-Log "Poetry: NOT FOUND (required for backend standalone mode)" -Level Warning
            Write-Log "Install: (Invoke-WebRequest -Uri https://install.python-poetry.org -UseBasicParsing).Content | py -" -Level Info
        }
    }

    if (-not $allGood) {
        throw "Missing required prerequisites"
    }
}

# ============================================================================
# Build Steps
# ============================================================================

function Invoke-CleanAll {
    if (-not $Clean) { return }

    Write-Log "Cleaning Build Artifacts" -Level Header

    $pathsToClean = @()

    if ($BuildComponents.tauri) {
        $pathsToClean += (Join-Path $TauriDir 'target')
    }
    if ($BuildComponents.frontend -or $BuildComponents.tauri) {
        $pathsToClean += $DistDir
        $pathsToClean += (Join-Path $ScriptDir 'node_modules' '.vite')
    }
    if ($BuildComponents.sidecar) {
        $pathsToClean += (Join-Path $PythonSidecarDir '__pycache__')
        $pathsToClean += (Join-Path $PythonSidecarDir '.pytest_cache')
    }
    if ($BuildComponents.backend) {
        $pathsToClean += (Join-Path $BackendDir '__pycache__')
        $pathsToClean += (Join-Path $BackendDir '.pytest_cache')
    }

    foreach ($path in $pathsToClean) {
        if (Test-Path $path) {
            Write-Log "Removing $path..." -Level Info
            Remove-Item -Recurse -Force $path -ErrorAction SilentlyContinue
        }
    }

    Write-Log "Cleanup complete" -Level Success
}

function Build-BackendServer {
    if (-not $BuildComponents.backend) { return }

    Write-Log "Building Backend Server" -Level Header

    if ($BackendMode -eq 'docker') {
        Write-Log "Using Docker Compose for backend services..." -Level Step

        # Check if docker-compose file exists
        $composeFile = Join-Path $BackendDir 'docker' 'docker-compose.dev.yml'
        if (-not (Test-Path $composeFile)) {
            throw "Docker Compose file not found: $composeFile"
        }

        # Stop existing containers
        Write-Log "Stopping existing containers..." -Level Info
        Push-Location $BackendDir
        docker compose -f $composeFile down
        Pop-Location

        # Build and start services
        Write-Log "Building and starting services..." -Level Info
        Push-Location $BackendDir
        docker compose -f $composeFile up -d --build
        if ($LASTEXITCODE -ne 0) {
            Pop-Location
            throw "Failed to build/start backend services"
        }
        Pop-Location

        # Wait for services to be healthy
        Write-Log "Waiting for services to be healthy..." -Level Info
        Start-Sleep -Seconds 10

        # Check service status
        Push-Location $BackendDir
        $status = docker compose -f $composeFile ps | Out-String
        Pop-Location
        Write-Log "Service status:`n$status" -Level Info

        Write-Log "Backend server started successfully" -Level Success
        Write-Log "  API: http://localhost:8000" -Level Info
        Write-Log "  Docs: http://localhost:8000/docs" -Level Info

    } else {
        Write-Log "Using standalone mode for backend..." -Level Step

        # Install dependencies with Poetry
        Write-Log "Installing Python dependencies..." -Level Info
        Invoke-CommandSafe "poetry install" -WorkingDirectory $BackendDir -ShowOutput -ErrorMessage "Failed to install dependencies"

        # Run migrations
        Write-Log "Running database migrations..." -Level Info
        Invoke-CommandSafe "poetry run alembic upgrade head" -WorkingDirectory $BackendDir -ShowOutput -ErrorMessage "Failed to run migrations"

        Write-Log "Backend setup complete (not started in background)" -Level Success
        Write-Log "To start: cd $BackendDir; poetry run uvicorn src.main:app --reload" -Level Info
    }
}

function Build-PythonSidecar {
    if (-not $BuildComponents.sidecar) { return }

    Write-Log "Building Python Sidecar" -Level Header

    # Check if Python backend exists
    if (-not (Test-Path $PythonSidecarDir)) {
        throw "Python sidecar directory not found: $PythonSidecarDir"
    }

    # Create virtual environment if it doesn't exist
    $venvDir = Join-Path $PythonSidecarDir 'venv'
    if (-not (Test-Path $venvDir)) {
        Write-Log "Creating Python virtual environment..." -Level Info
        Invoke-CommandSafe "python -m venv venv" -WorkingDirectory $PythonSidecarDir -ErrorMessage "Failed to create venv"
    }

    # Install dependencies
    Write-Log "Installing Python sidecar dependencies..." -Level Info
    $requirementsFile = Join-Path $PythonSidecarDir 'requirements.txt'
    if (Test-Path $requirementsFile) {
        Push-Location $PythonSidecarDir
        try {
            .\venv\Scripts\pip.exe install -r requirements.txt
            if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
                throw "pip install failed with exit code $LASTEXITCODE"
            }
            Write-Log "Python dependencies installed successfully" -Level Success
        } catch {
            Pop-Location
            throw "Failed to install sidecar dependencies: $_"
        }
        Pop-Location
    }

    # Bundle sidecar if requested
    if ($BundleSidecar) {
        Write-Log "Bundling Python sidecar as executable..." -Level Step

        # Check if PyInstaller is installed
        try {
            Write-Log "Installing PyInstaller..." -Level Info
            Push-Location $PythonSidecarDir
            .\venv\Scripts\pip.exe install pyinstaller
            Pop-Location
            if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
                throw "PyInstaller installation failed"
            }

            # Run bundle script
            $bundleScript = Join-Path $ScriptDir 'scripts' 'bundle_python.py'
            if (Test-Path $bundleScript) {
                Write-Log "Running bundle script..." -Level Info
                Push-Location $PythonSidecarDir
                ..\venv\Scripts\python.exe ..\scripts\bundle_python.py
                Pop-Location
                if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
                    throw "Bundle script failed"
                }
                Write-Log "Python sidecar bundled successfully" -Level Success
            } else {
                Write-Log "Bundle script not found, copying source files instead..." -Level Warning
                Build-CopySidecarFiles
            }
        } catch {
            Write-Log "Failed to bundle sidecar: $_" -Level Warning
            Write-Log "Falling back to copying source files..." -Level Info
            Build-CopySidecarFiles
        }
    } else {
        Build-CopySidecarFiles
    }

    Write-Log "Python sidecar ready" -Level Success
}

function Build-CopySidecarFiles {
    Write-Log "Copying Python sidecar files to Tauri resources..." -Level Info

    $resourcesDir = "src-tauri\resources\python_backend"
    New-Item -ItemType Directory -Force -Path $resourcesDir | Out-Null

    # Copy Python files
    Push-Location $ScriptDir
    $filesToCopy = @('*.py', 'requirements.txt', 'qa', 'embeddings', 'search')
    foreach ($item in $filesToCopy) {
        $sourcePath = "python_backend\$item"
        if (Test-Path $sourcePath) {
            Copy-Item -Path $sourcePath -Destination $resourcesDir -Recurse -Force
        }
    }
    Pop-Location

    Write-Log "Python sidecar files copied" -Level Success
}

function Build-Frontend {
    if (-not $BuildComponents.frontend) { return }

    Write-Log "Building Frontend" -Level Header

    # Install dependencies if needed
    $nodeModulesPath = Join-Path $ScriptDir "node_modules"
    if (-not (Test-Path $nodeModulesPath)) {
        Write-Log "Installing frontend dependencies..." -Level Info
        Invoke-CommandSafe "npm install" -WorkingDirectory $ScriptDir -ShowOutput -ErrorMessage "Failed to install frontend dependencies"
    } else {
        Write-Log "Dependencies already installed (skipping npm install)" -Level Info
    }

    # Build frontend
    if ($Mode -eq 'dev') {
        Write-Log "Skipping frontend build in dev mode (uses Vite dev server)" -Level Info
    } else {
        Write-Log "Building React frontend..." -Level Info
        $env:NODE_ENV = if ($Mode -eq 'production') { 'production' } else { 'production' }
        Push-Location $ScriptDir
        try {
            npx vite build
            if ($LASTEXITCODE -ne 0 -and $null -ne $LASTEXITCODE) {
                throw "Vite build failed with exit code $LASTEXITCODE"
            }
            Write-Log "Frontend build complete" -Level Success
        } catch {
            Pop-Location
            throw "Failed to build frontend: $_"
        }
        Pop-Location
    }
}

function Build-TauriApp {
    if (-not $BuildComponents.tauri) { return }

    Write-Log "Building Tauri Application" -Level Header

    # Build Tauri app
    $buildArgs = @()

    if ($Mode -eq 'dev') {
        Write-Log "Building in development mode..." -Level Info
        $buildArgs += '--debug'
    } else {
        Write-Log "Building in release mode..." -Level Info
    }

    # Build command
    if ($Mode -eq 'dev') {
        Write-Log "Note: Use 'npm run tauri:dev' for development with hot reload" -Level Info
        Invoke-CommandSafe "cargo build" -WorkingDirectory $TauriDir -ShowOutput -ErrorMessage "Failed to build Tauri backend"
    } else {
        $cmd = "npm run tauri:build"
        if ($buildArgs.Count -gt 0) {
            $cmd += " -- " + ($buildArgs -join ' ')
        }
        Invoke-CommandSafe $cmd -WorkingDirectory $ScriptDir -ShowOutput -ErrorMessage "Failed to build Tauri app"
    }

    Write-Log "Tauri application build complete" -Level Success
}

function New-Checksums {
    if (-not $Checksums) { return }

    Write-Log "Generating Checksums" -Level Header

    $bundleSubdir = if ($Mode -eq 'dev') { 'debug' } else { 'release' }
    $bundleDir = Join-Path $TauriDir "target\$bundleSubdir\bundle"

    if (-not (Test-Path $bundleDir)) {
        Write-Log "Bundle directory not found, skipping checksums" -Level Warning
        return
    }

    $checksumFile = Join-Path $DistDir 'checksums.txt'
    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

    $checksums = @()
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

    Write-Host "`nComponents built:" -ForegroundColor White
    if ($BuildComponents.backend) { Write-Host "  [OK] Backend Server ($BackendMode mode)" -ForegroundColor Green }
    if ($BuildComponents.sidecar) { Write-Host "  [OK] Python Sidecar $(if($BundleSidecar){'(bundled)'}else{'(source)'})" -ForegroundColor Green }
    if ($BuildComponents.frontend) { Write-Host "  [OK] Frontend (React + Vite)" -ForegroundColor Green }
    if ($BuildComponents.tauri) { Write-Host "  [OK] Tauri Desktop App (Rust + React)" -ForegroundColor Green }

    # Show output locations
    if ($BuildComponents.tauri) {
        $targetDir = Join-Path $TauriDir "target\$(if($Mode -eq 'dev'){'debug'}else{'release'})"
        if (Test-Path $targetDir) {
            Write-Host "`nBuild artifacts:" -ForegroundColor White
            Write-Host "  Location: $targetDir" -ForegroundColor Cyan

            $bundleDir = Join-Path $targetDir 'bundle'
            if (Test-Path $bundleDir) {
                Write-Host "`nGenerated installers:" -ForegroundColor White
                Get-ChildItem -Path $bundleDir -Recurse -File -Include *.msi, *.exe |
                    ForEach-Object {
                        $size = [math]::Round($_.Length / 1MB, 2)
                        Write-Host "  - $($_.Name) (${size} MB)" -ForegroundColor Cyan
                    }
            }
        }
    }

    if ($BuildComponents.backend) {
        Write-Host "`nBackend Server:" -ForegroundColor White
        if ($BackendMode -eq 'docker') {
            Write-Host "  API: http://localhost:8000" -ForegroundColor Cyan
            Write-Host "  Docs: http://localhost:8000/docs" -ForegroundColor Cyan
            Write-Host "  Status: docker compose -f $BackendDir/docker/docker-compose.dev.yml ps" -ForegroundColor Gray
        } else {
            Write-Host "  Start: cd $BackendDir; poetry run uvicorn src.main:app --reload" -ForegroundColor Cyan
        }
    }

    Write-Host "`nBuild log: $LogFile" -ForegroundColor Gray
}

# ============================================================================
# Main Build Process
# ============================================================================

function Start-MasterBuild {
    try {
        Write-Host @"

╔══════════════════════════════════════════════════════════╗
║                                                          ║
║        Vault Desktop - Master Build System              ║
║                                                          ║
╚══════════════════════════════════════════════════════════╝

"@ -ForegroundColor Cyan

        Write-Log "Vault Master Build System" -Level Header
        Write-Log "Mode: $Mode" -Level Info
        Write-Log "Components: $Components" -Level Info
        if ($BuildComponents.backend) {
            Write-Log "Backend Mode: $BackendMode" -Level Info
        }
        if ($BuildComponents.sidecar) {
            Write-Log "Bundle Sidecar: $BundleSidecar" -Level Info
        }

        Test-Prerequisites
        Invoke-CleanAll
        Build-BackendServer
        Build-PythonSidecar
        Build-Frontend
        Build-TauriApp
        New-Checksums
        Show-Summary

        Write-Log "Build Completed Successfully! 🎉" -Level Header

    } catch {
        Write-Log "Build failed: $_" -Level Error
        Write-Log "Check log file for details: $LogFile" -Level Error

        if ($ShowVerbose) {
            Write-Host $_.Exception.ToString() -ForegroundColor Red
            Write-Host $_.ScriptStackTrace -ForegroundColor Red
        }

        exit 1
    }
}

# ============================================================================
# Entry Point
# ============================================================================

Start-MasterBuild

Write-Host "`n✨ All done! ✨`n" -ForegroundColor Green

