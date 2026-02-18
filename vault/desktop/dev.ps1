# Development server launcher with proper VS environment
$ErrorActionPreference = "Continue"

Write-Host "=== Starting Recall Desktop Development Mode ===" -ForegroundColor Cyan

# Set VS environment variables
$env:VSINSTALLDIR = "C:\Program Files\Microsoft Visual Studio\2022\Professional\"
$env:VCINSTALLDIR = "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\"
$env:VCToolsVersion = "14.44.35207"
$env:WindowsSdkVersion = "10.0.26100.0"

# Set library paths
$env:LIB = "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64"

# Set include paths
$env:INCLUDE = "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared"

# Prepend VS tools and Cargo to PATH
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64;C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64;" + $env:PATH

Write-Host "Environment configured for Visual Studio 2022" -ForegroundColor Green
Write-Host "Cargo in PATH: " -NoNewline
Write-Host (Get-Command cargo -ErrorAction SilentlyContinue).Path -ForegroundColor Yellow
Write-Host ""

# Navigate to project directory
Set-Location "C:\Code\Recall\vault\desktop"

# Start Tauri dev server
Write-Host "Starting Tauri development server..." -ForegroundColor Cyan
Write-Host ""

npm run tauri:dev
