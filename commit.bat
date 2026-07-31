@echo off
setlocal EnableExtensions

rem Always run Codex against the repository that contains this batch file.
set "REPO_DIR=%~dp0."
set "CODEX_PROMPT_FILE=%TEMP%\superexplorer-codex-prompt-%RANDOM%-%RANDOM%.txt"

where codex >nul 2>&1
if errorlevel 1 (
    echo Codex CLI was not found in PATH.
    exit /b 9009
)

rem Keep this prompt ASCII-only so cmd.exe can parse the batch file reliably.
rem Codex is explicitly instructed to write every Git commit message in Chinese.
> "%CODEX_PROMPT_FILE%" (
    echo Execute the complete Git commit and push workflow now. Do not only describe a plan.
    echo Do not edit, generate, format, or delete any existing project file.
    echo You may only inspect existing changes, stage them, commit them, and push them.
    echo Inspect the parent repository and every initialized submodule, including nested submodules.
    echo Review staged changes, unstaged changes, and untracked files before committing anything.
    echo Exclude compilation outputs, build outputs, test artifacts, caches, logs, package outputs, and tool-generated temporary files.
    echo Do not modify .gitignore to hide generated files.
    echo Group eligible changes into separate commits according to feature and functional relationship.
    echo Never combine unrelated functionality in the same commit.
    echo Every commit subject and detailed commit body must be written in Traditional Chinese.
    echo Each commit body must explain the changes, purpose, and important effects.
    echo Review every staged diff before committing and exclude secrets or unrelated content.
    echo For each changed submodule, commit eligible changes inside that submodule before updating the parent repository pointer.
    echo Inspect submodule remotes before pushing. Push only to a writable fork remote, normally origin.
    echo Never attempt to push to a read-only upstream remote.
    echo If a submodule branch tracks upstream, push HEAD to the same branch name on origin instead.
    echo Push all changed submodules successfully before committing their updated pointers in the parent repository.
    echo After all parent repository commits are complete, run git push origin HEAD:master.
    echo The required final destination for the parent repository is origin/master.
    echo If there are no eligible changes, report that clearly without creating an empty commit.
    echo If a remote, branch, permission, or safety problem cannot be resolved without editing files or rewriting history, stop that unsafe action and report the exact problem.
    echo Continue working until all eligible commits and required pushes have completed or a concrete blocking error occurs.
    echo Finish with a list of created commits, intentionally uncommitted files, and every push result.
)

if not exist "%CODEX_PROMPT_FILE%" (
    echo Failed to create the Codex prompt file.
    exit /b 1
)

for %%A in ("%CODEX_PROMPT_FILE%") do if %%~zA EQU 0 (
    echo The Codex prompt file is empty.
    del /q "%CODEX_PROMPT_FILE%" >nul 2>&1
    exit /b 1
)

rem CALL is required because npm installs Codex as codex.cmd on Windows.
call codex -a never -s danger-full-access -m gpt-5.3-codex-spark -c model_reasoning_effort="low" -C "%REPO_DIR%" exec --ignore-user-config --ignore-rules - < "%CODEX_PROMPT_FILE%"
set "CODEX_EXIT_CODE=%ERRORLEVEL%"

del /q "%CODEX_PROMPT_FILE%" >nul 2>&1

if not "%CODEX_EXIT_CODE%"=="0" (
    echo Codex failed with exit code %CODEX_EXIT_CODE%.
    echo Review the error above, then press any key to close this window.
    pause >nul
) else (
    echo Codex completed the commit and push workflow.
)

exit /b %CODEX_EXIT_CODE%
