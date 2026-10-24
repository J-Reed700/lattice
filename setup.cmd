@echo off
rem Double-click to set up Lattice on Windows. Runs setup.ps1; see that file.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup.ps1" %*
echo.
pause
