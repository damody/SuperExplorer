@echo off
setlocal
"%SystemRoot%\System32\chcp.com" 65001 >nul 2>&1

set "PATH=%USERPROFILE%\.cargo\bin;%SystemRoot%\System32;%SystemRoot%\System32\WindowsPowerShell\v1.0;%PATH%"

set "LUA_EXE=%~dp0build\tools\lua\lua.exe"
set "BUILD_SCRIPT=%~dp0build\build_install.lua"
set "BUILD_EXIT_CODE=1"
set "CHECK_ONLY=0"
set "NO_LAUNCH=0"
set "PAUSE_ON_FAILURE=0"

if "%~1"=="" if not defined CI set "PAUSE_ON_FAILURE=1"

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

if defined CI goto :run_build
if "%CHECK_ONLY%"=="1" goto :run_build
if "%NO_LAUNCH%"=="1" goto :run_build
if defined SUPEREXPLORER_TEST_INSTALL_BOOTSTRAPPED goto :run_build

set "SUPEREXPLORER_TEST_INSTALL_BOOTSTRAPPED=1"
echo [INFO] Relaunching SuperExplorer test install in a detached console so SuperExplorer can stay open until packaging finishes.
start "SuperExplorer Test Install" cmd /d /c call "%~f0" %*
exit /b 0

:run_build
"%LUA_EXE%" "%BUILD_SCRIPT%" --component superexplorer --allow-superexplorer-dirty --auto-install %*
set "BUILD_EXIT_CODE=%ERRORLEVEL%"

:finish
echo.
if not "%BUILD_EXIT_CODE%"=="0" goto :report_failure
if "%CHECK_ONLY%"=="1" goto :report_check
if "%NO_LAUNCH%"=="1" goto :report_built
echo [SUCCESS] SuperExplorer test installer build completed, installed, verified, and launched.
goto :report_done

:report_built
echo [SUCCESS] SuperExplorer test installer build completed without launching it.
goto :report_done

:report_check
echo [SUCCESS] SuperExplorer test installer build check completed; no installer was created or launched.
goto :report_done

:report_failure
echo [FAILURE] SuperExplorer test installer build failed with exit code %BUILD_EXIT_CODE%. 1>&2
if not "%PAUSE_ON_FAILURE%"=="1" goto :report_done
echo.
echo [FAILURE] Press any key to close this window. 1>&2
echo [DIAGNOSTICS] Client errors are also persisted under %LOCALAPPDATA%\RustGpuiExplorer\logs\error.log.
pause >nul

:report_done
echo.

exit /b %BUILD_EXIT_CODE%
