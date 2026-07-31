@echo off
setlocal EnableExtensions DisableDelayedExpansion
chcp 65001 >nul

set "WORKSPACE=D:\test"
set "LUA_EXE=D:\test\build\tools\lua\lua.exe"
set "REPORT_SCRIPT=D:\test\scripts\utit_report.lua"

if not exist "%LUA_EXE%" (
    echo [UTIT] Lua executable not found: %LUA_EXE%
    exit /b 2
)
if not exist "%REPORT_SCRIPT%" (
    echo [UTIT] Report script not found: %REPORT_SCRIPT%
    exit /b 2
)

cd /d "%WORKSPACE%" || exit /b 2
set "RUN_TAG=%RANDOM%-%RANDOM%"
set "RUN_DIRECTORY=target\uitest-runs\utit-%RUN_TAG%"
set "RAW_LOG=target\uitest-runs\utit-console-%RUN_TAG%.log"
if not exist "target\uitest-runs" mkdir "target\uitest-runs" || exit /b 2

set "NO_COLOR=1"
set "CARGO_TERM_COLOR=never"

if "%~1"=="" (
    echo [UTIT] Running quick, full, interop and visual suites...
    cargo run -p explorer-uitest -- --suite quick --suite full --suite interop --suite visual --output "%RUN_DIRECTORY%" >"%RAW_LOG%" 2>&1
) else (
    echo [UTIT] Running selected UITEST arguments: %*
    cargo run -p explorer-uitest -- %* --output "%RUN_DIRECTORY%" >"%RAW_LOG%" 2>&1
)
set "RUNNER_EXIT=%ERRORLEVEL%"

"%LUA_EXE%" "%REPORT_SCRIPT%" "%WORKSPACE%" "%RUN_DIRECTORY%" "%RAW_LOG%" "%RUNNER_EXIT%"
set "REPORT_EXIT=%ERRORLEVEL%"

if not "%REPORT_EXIT%"=="0" exit /b %REPORT_EXIT%
exit /b %RUNNER_EXIT%
