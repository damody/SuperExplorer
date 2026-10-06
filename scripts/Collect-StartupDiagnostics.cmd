@echo off
setlocal
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File "%~dp0Collect-StartupDiagnostics.ps1" %*
echo.
pause
