@echo off
echo === Starting Lattice Desktop App ===
cd C:\Code\Lattice\lattice\desktop

:: Set Visual Studio environment
set "VSINSTALLDIR=C:\Program Files\Microsoft Visual Studio\2022\Professional\"
set "VCINSTALLDIR=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\"
set "VCToolsVersion=14.44.35207"
set "WindowsSdkVersion=10.0.26100.0"

:: Set library paths
set "LIB=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\lib\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\ucrt\x64;C:\Program Files (x86)\Windows Kits\10\Lib\10.0.26100.0\um\x64"

:: Set include paths
set "INCLUDE=C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\include;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\ucrt;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\um;C:\Program Files (x86)\Windows Kits\10\Include\10.0.26100.0\shared"

:: Prepend VS tools and Cargo to PATH
set "PATH=%USERPROFILE%\.cargo\bin;C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Tools\MSVC\14.44.35207\bin\Hostx64\x64;C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64;%PATH%"

echo Environment configured for Visual Studio 2022
echo.
echo Starting Tauri development server...
echo This will compile the Rust code and launch the desktop app.
echo Please wait, this may take a few minutes...
echo.

:: Run Tauri dev server - this will stay running
npm run tauri:dev

:: If it exits, pause to see any error
pause