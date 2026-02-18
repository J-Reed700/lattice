#!/usr/bin/env pwsh
# Script to prepare SQLx offline mode by generating query metadata

$ErrorActionPreference = "Stop"

Write-Host "=== SQLx Offline Mode Preparation ===" -ForegroundColor Cyan

# Paths
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$TempDb = Join-Path $ScriptDir ".sqlx-temp.db"
$SchemaFile = Join-Path $ScriptDir "src\db\schema.sql"

# Clean up any existing temp database
if (Test-Path $TempDb) {
    Write-Host "Removing old temporary database..." -ForegroundColor Yellow
    Remove-Item $TempDb -Force
}

# Check if schema file exists
if (-not (Test-Path $SchemaFile)) {
    Write-Error "Schema file not found at: $SchemaFile"
    exit 1
}

Write-Host "Creating temporary SQLite database..." -ForegroundColor Green
Write-Host "  Location: $TempDb" -ForegroundColor Gray

# Create database and run schema
try {
    # Create empty database file
    New-Item -ItemType File -Path $TempDb -Force | Out-Null
    
    # Apply schema using sqlite3
    $schemaContent = Get-Content $SchemaFile -Raw
    
    # Try to find sqlite3 executable
    $sqlite3 = $null
    $possiblePaths = @(
        "sqlite3",
        "sqlite3.exe",
        "C:\Program Files\SQLite\sqlite3.exe",
        "$env:LOCALAPPDATA\Programs\SQLite\sqlite3.exe"
    )
    
    foreach ($path in $possiblePaths) {
        if (Get-Command $path -ErrorAction SilentlyContinue) {
            $sqlite3 = $path
            break
        }
    }
    
    if ($sqlite3) {
        Write-Host "Using sqlite3 to create schema..." -ForegroundColor Green
        $schemaContent | & $sqlite3 $TempDb
    } else {
        Write-Host "sqlite3 not found, using .NET SQLite..." -ForegroundColor Yellow
        
        # Use .NET System.Data.SQLite as fallback
        Add-Type -Path "System.Data.SQLite.dll" -ErrorAction SilentlyContinue
        
        $connectionString = "Data Source=$TempDb;Version=3;"
        $connection = New-Object System.Data.SQLite.SQLiteConnection($connectionString)
        $connection.Open()
        
        $command = $connection.CreateCommand()
        $command.CommandText = $schemaContent
        $command.ExecuteNonQuery() | Out-Null
        
        $connection.Close()
        Write-Host "Schema applied successfully using .NET SQLite" -ForegroundColor Green
    }
    
    Write-Host "Database created successfully!" -ForegroundColor Green
    
} catch {
    Write-Error "Failed to create database: $_"
    exit 1
}

# Set DATABASE_URL environment variable
$DatabaseUrl = "sqlite://$TempDb"
Write-Host "`nSetting DATABASE_URL=$DatabaseUrl" -ForegroundColor Green
$env:DATABASE_URL = $DatabaseUrl

# Run cargo sqlx prepare
Write-Host "`nRunning 'cargo sqlx prepare'..." -ForegroundColor Cyan
try {
    Push-Location $ScriptDir
    cargo sqlx prepare --workspace
    if ($LASTEXITCODE -eq 0) {
        Write-Host "`n[SUCCESS] SQLx metadata generated!" -ForegroundColor Green
        Write-Host "  - Metadata saved to .sqlx/ directory" -ForegroundColor Gray
        Write-Host "  - Offline compilation is now enabled" -ForegroundColor Gray
    } else {
        throw "cargo sqlx prepare failed with exit code $LASTEXITCODE"
    }
} catch {
    Write-Error "Failed to generate SQLx metadata: $_"
    exit 1
} finally {
    Pop-Location
}

# Clean up temporary database
Write-Host "`nCleaning up temporary database..." -ForegroundColor Yellow
if (Test-Path $TempDb) {
    Remove-Item $TempDb -Force
}

Write-Host "`n=== SQLx Preparation Complete! ===" -ForegroundColor Green
Write-Host "You can now build without DATABASE_URL set." -ForegroundColor Cyan

