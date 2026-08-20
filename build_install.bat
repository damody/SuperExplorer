@echo off
setlocal
"%SystemRoot%\System32\chcp.com" 65001 >nul 2>&1

set "LUA_EXE=%~dp0build\tools\lua\lua.exe"
set "BUILD_SCRIPT=%~dp0build\build_install.lua"
set "BUILD_EXIT_CODE=1"
set "CHECK_ONLY=0"
set "NO_LAUNCH=0"
set "PAUSE_ON_FAILURE=0"

if "%~1"=="" if not defined CI set "PAUSE_ON_FAILURE=1"

set "GIT_EXE="
for %%G in (git.exe) do if not "%%~$PATH:G"=="" set "GIT_EXE=%%~$PATH:G"
if not defined GIT_EXE if exist "%ProgramFiles%\Git\cmd\git.exe" set "GIT_EXE=%ProgramFiles%\Git\cmd\git.exe"
if not defined GIT_EXE if exist "%ProgramFiles%\Git\bin\git.exe" set "GIT_EXE=%ProgramFiles%\Git\bin\git.exe"
if not defined GIT_EXE if exist "%ProgramFiles(x86)%\Git\cmd\git.exe" set "GIT_EXE=%ProgramFiles(x86)%\Git\cmd\git.exe"
if not defined GIT_EXE if exist "%ProgramFiles(x86)%\Git\bin\git.exe" set "GIT_EXE=%ProgramFiles(x86)%\Git\bin\git.exe"

if not defined GIT_EXE (
    echo [錯誤] 找不到 Git。請安裝 Git for Windows，或將 git.exe 加入 PATH。 1>&2
    goto :finish
)

for %%G in ("%GIT_EXE%") do set "GIT_BIN_DIR=%%~dpG"
set "PATH=%GIT_BIN_DIR%;%SystemRoot%\System32;%SystemRoot%\System32\WindowsPowerShell\v1.0;%USERPROFILE%\.cargo\bin;%PATH%"

for %%A in (%*) do (
    if /I "%%~A"=="--check" set "CHECK_ONLY=1"
    if /I "%%~A"=="--no-launch" set "NO_LAUNCH=1"
)

if not exist "%LUA_EXE%" (
    echo [錯誤] 找不到專案隨附的 Lua 執行環境：%LUA_EXE% 1>&2
    goto :finish
)

if not exist "%BUILD_SCRIPT%" (
    echo [錯誤] 找不到安裝程式建置腳本：%BUILD_SCRIPT% 1>&2
    goto :finish
)

"%LUA_EXE%" "%BUILD_SCRIPT%" --component all --ignore-superdesktop-evidence-logs %*
set "BUILD_EXIT_CODE=%ERRORLEVEL%"

:finish
echo.
if not "%BUILD_EXIT_CODE%"=="0" goto :report_failure
if "%CHECK_ONLY%"=="1" goto :report_check
if "%NO_LAUNCH%"=="1" goto :report_built
echo [SUCCESS] Installer build completed and launched.
goto :report_done

:report_built
echo [SUCCESS] Installer build completed without launching it.
goto :report_done

:report_check
echo [SUCCESS] Installer build check completed; no installer was created or launched.
goto :report_done

:report_failure
echo [FAILURE] Installer build failed with exit code %BUILD_EXIT_CODE%. 1>&2
if not "%PAUSE_ON_FAILURE%"=="1" goto :report_done
echo.
echo [FAILURE] Press any key to close this window. 1>&2
pause >nul

:report_done
echo.

exit /b %BUILD_EXIT_CODE%
