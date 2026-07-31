@echo off
setlocal
chcp 65001 >nul

set "LUA_EXE=D:\test\build\tools\lua\lua.exe"
set "BUILD_SCRIPT=%~dp0build\build_install.lua"
set "BUILD_EXIT_CODE=1"
set "CHECK_ONLY=0"

for %%A in (%*) do (
    if /I "%%~A"=="--check" set "CHECK_ONLY=1"
)

if not exist "%LUA_EXE%" (
    echo [錯誤] 找不到專案隨附的 Lua 執行環境：%LUA_EXE% 1>&2
    goto :finish
)

if not exist "%BUILD_SCRIPT%" (
    echo [錯誤] 找不到安裝程式建置腳本：%BUILD_SCRIPT% 1>&2
    goto :finish
)

"%LUA_EXE%" "%BUILD_SCRIPT%" %*
set "BUILD_EXIT_CODE=%ERRORLEVEL%"

:finish
echo.
if not "%BUILD_EXIT_CODE%"=="0" goto :report_failure
if "%CHECK_ONLY%"=="1" goto :report_check
echo [SUCCESS] Installer build completed and launched.
goto :report_done

:report_check
echo [SUCCESS] Installer build check completed; no installer was created or launched.
goto :report_done

:report_failure
echo [FAILURE] Installer build failed with exit code %BUILD_EXIT_CODE%. 1>&2

:report_done
echo.

exit /b %BUILD_EXIT_CODE%
