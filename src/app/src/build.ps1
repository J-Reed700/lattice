# PowerShell build script with VS environment
$ErrorActionPreference = "Continue"

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

Write-Host "Environment configured for Visual Studio 2022"
Write-Host "LIB = $env:LIB"
Write-Host ""

# Change to project directory
Set-Location "C:\Code\Recall\vault\desktop\src-tauri"

# Run cargo check
Write-Host "Running cargo check --all-features..."
Write-Host ""
& cargo check --all-features

Write-Host ""
Write-Host "Build completed with exit code: $LASTEXITCODE"
exit $LASTEXITCODE
