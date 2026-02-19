@echo off
echo === Launching Recall Desktop ===
cd C:\Code\Recall\vault\desktop

:: Set VS environment
set "VSINSTALLDIR=C:\Program Files\Microsoft Visual Studio\2022\Professional\"
set "VCINSTALLDIR=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\"
set "VCToolsVersion=14.44.35207"
set "WindowsSdkVersion=10.0.26100.0"
set "LIB=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64"
set "INCLUDE=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared"
set "PATH=%USERPROFILE%\.cargo\bin;C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64;C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64;%PATH%"

echo Environment configured
echo.
echo Starting Tauri app...
npm run tauri:dev