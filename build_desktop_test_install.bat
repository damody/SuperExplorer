@echo off
setlocal
chcp 65001 >nul

set "LUA_EXE=%~dp0build\tools\lua\lua.exe"
set "BUILD_SCRIPT=%~dp0build\build_install.lua"
set "BUILD_EXIT_CODE=1"
set "CHECK_ONLY=0"
set "NO_LAUNCH=0"

for %%A in (%*) do (
    if /I "%%~A"=="--check" set "CHECK_ONLY=1"
    if /I "%%~A"=="--no-launch" set "NO_LAUNCH=1"
)

if not exist "%LUA_EXE%" (
    echo [ERROR] Bundled Lua runtime was not found: %LUA_EXE% 1>&2
    goto :finish
)

if not exist "%BUILD_SCRIPT%" (
    echo [ERROR] Installer build script was not found: %BUILD_SCRIPT% 1>&2
    goto :finish
)

"%LUA_EXE%" "%BUILD_SCRIPT%" --component superdesktop --allow-superdesktop-dirty %*
set "BUILD_EXIT_CODE=%ERRORLEVEL%"

:finish
echo.
if not "%BUILD_EXIT_CODE%"=="0" goto :report_failure
if "%CHECK_ONLY%"=="1" goto :report_check
if "%NO_LAUNCH%"=="1" goto :report_built
echo [SUCCESS] SuperDesktop test installer build completed and launched.
goto :report_done

:report_built
echo [SUCCESS] SuperDesktop test installer build completed without launching it.
goto :report_done

:report_check
echo [SUCCESS] SuperDesktop test installer build check completed; no installer was created or launched.
goto :report_done

:report_failure
echo [FAILURE] SuperDesktop test installer build failed with exit code %BUILD_EXIT_CODE%. 1>&2

:report_done
echo.

exit /b %BUILD_EXIT_CODE%
