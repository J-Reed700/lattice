@echo off
call "%ProgramFiles%\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvars64.bat"
cd /d "C:\Code\Recall\vault\desktop\src-tauri"
echo Running cargo check...
cargo check --all-features
echo.
echo Build completed with error level: %ERRORLEVEL%
