@echo off
call "C:\Program Files\Microsoft Visual Studio\2022\Professional\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 >nul 2>&1
cd /d "C:\Code\Recall\vault\desktop\src-tauri"
set PATH=%USERPROFILE%\.cargo\bin;%PATH%
REM Temporarily rename Git's link.exe to avoid conflict
if exist "C:\Program Files\Git\usr\bin\link.exe" ren "C:\Program Files\Git\usr\bin\link.exe" link.exe.backup
cargo check --all-features 2>&1
REM Restore Git's link.exe
if exist "C:\Program Files\Git\usr\bin\link.exe.backup" ren "C:\Program Files\Git\usr\bin\link.exe.backup" link.exe
