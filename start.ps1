# Simple development server launcher
$ErrorActionPreference = "Continue"

Write-Host "=== Starting Lattice Desktop (Frontend Only) ===" -ForegroundColor Cyan

# Set VS environment variables
$env:VSINSTALLDIR = "C:\Program Files\Microsoft Visual Studio\2022\Professional\"
$env:VCINSTALLDIR = "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\"
$env:VCToolsVersion = "14.44.35207"
$env:WindowsSdkVersion = "10.0.26100.0"
$env:LIB = "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64"
$env:INCLUDE = "C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared"
$env:PATH = "$env:USERPROFILE\.cargo\bin;C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64;C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64;" + $env:PATH

Write-Host "Environment configured" -ForegroundColor Green
Set-Location "C:\Code\Lattice\lattice\desktop"

Write-Host "`nStarting frontend (Vite) in background..." -ForegroundColor Cyan
$viteProcess = Start-Process npx -ArgumentList "vite" -PassThru -WindowStyle Hidden

Start-Sleep -Seconds 3

Write-Host "Starting Tauri app..." -ForegroundColor Cyan
& npm run tauri:dev

# Cleanup
if ($viteProcess -and !$viteProcess.HasExited) {
    $viteProcess.Kill()
}
