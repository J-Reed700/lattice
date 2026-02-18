#!/usr/bin/env pwsh
# Script to generate SQLx offline metadata (sqlx-data.json)

$ErrorActionPreference = "Stop"

Write-Host "=== SQLx Offline Metadata Generation ===" -ForegroundColor Cyan

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$TempDb = Join-Path $env:TEMP "sqlx-recall-$(Get-Random).db"
$SchemaFile = Join-Path $ScriptDir "src\db\schema.sql"

# Check if sqlite3 is available
Write-Host "Checking for sqlite3..." -ForegroundColor Cyan
if (-not (Get-Command sqlite3 -ErrorAction SilentlyContinue)) {
    Write-Error @"
ERROR: sqlite3 command not found!

Please install SQLite command-line tools:
  1. Download from: https://www.sqlite.org/download.html
  2. Extract sqlite3.exe to a directory
  3. Add that directory to your PATH
  4. Open a NEW PowerShell window and try again

Or use Windows Package Manager:
  winget install SQLite.SQLite
"@
    exit 1
}

Write-Host "[OK] sqlite3 found: $(( Get-Command sqlite3).Source)" -ForegroundColor Green

# Create temp database using sqlite3
Write-Host "`nCreating temporary SQLite database..." -ForegroundColor Green
Write-Host "  Location: $TempDb" -ForegroundColor Gray

try {
    # Run sqlite3 to create database and apply schema
    Get-Content $SchemaFile -Raw | sqlite3 $TempDb
    
    if ($LASTEXITCODE -ne 0) {
        throw "sqlite3 failed with exit code $LASTEXITCODE"
    }
    
    Write-Host "Database schema created successfully" -ForegroundColor Green
    
} catch {
    Write-Error "Failed to create database: $_"
    if (Test-Path $TempDb) { Remove-Item $TempDb -Force }
    exit 1
}

# Set DATABASE_URL
$DatabaseUrl = "sqlite://$TempDb"
$env:DATABASE_URL = $DatabaseUrl
Write-Host "`nDATABASE_URL set to: $DatabaseUrl" -ForegroundColor Green

# Run cargo sqlx prepare
Write-Host "`nRunning 'cargo sqlx prepare --workspace'..." -ForegroundColor Cyan
Write-Host "(This will generate .sqlx/query-*.json files)" -ForegroundColor Gray

try {
    Push-Location $ScriptDir
    
    # Try to run cargo
    $cargoResult = cargo sqlx prepare --workspace 2>&1
    
    if ($LASTEXITCODE -eq 0) {
        Write-Host "`n[SUCCESS] SQLx metadata generated!" -ForegroundColor Green
        Write-Host "  - Metadata files saved to .sqlx/ directory" -ForegroundColor Gray
        Write-Host "  - You can now build without DATABASE_URL" -ForegroundColor Gray
        Write-Host "  - Commit the .sqlx/ directory to version control" -ForegroundColor Yellow
    } else {
        Write-Error "cargo sqlx prepare failed with exit code $LASTEXITCODE"
        Write-Host "Output: $cargoResult" -ForegroundColor Red
        throw "Failed to generate metadata"
    }
    
} catch {
    Write-Error "Failed to run cargo sqlx prepare: $_"
    Write-Host "`nTroubleshooting:" -ForegroundColor Yellow
    Write-Host "  1. Make sure Rust/Cargo is installed and in PATH" -ForegroundColor Gray
    Write-Host "  2. Try running this in a fresh PowerShell window" -ForegroundColor Gray
    Write-Host "  3. Or run manually:" -ForegroundColor Gray
    Write-Host "     `$env:DATABASE_URL='$DatabaseUrl'; cargo sqlx prepare --workspace" -ForegroundColor Gray
    exit 1
} finally {
    Pop-Location
}

# Clean up temp database
Write-Host "`nCleaning up temporary database..." -ForegroundColor Yellow
if (Test-Path $TempDb) {
    Remove-Item $TempDb -Force
    Write-Host "Temporary database removed" -ForegroundColor Green
}

Write-Host "`n=== SQLx Offline Metadata Generation Complete! ===" -ForegroundColor Green
Write-Host "`nNext steps:" -ForegroundColor Cyan
Write-Host "  1. Verify .sqlx/ directory was created in src-tauri/" -ForegroundColor Gray
Write-Host "  2. Commit .sqlx/ directory to git" -ForegroundColor Gray
Write-Host "  3. Build will now work without DATABASE_URL" -ForegroundColor Gray

