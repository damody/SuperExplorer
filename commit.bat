@echo off
setlocal

rem The prompt is UTF-8 text encoded as Base64 so cmd.exe only parses ASCII.
set "CODEX_PROMPT_B64=5LiN6KaB5L+u5pS55Lu75L2V54++5pyJ5qqU5qGI77yM5Y+q6IO95YiG5p6Q44CB5pqr5a2Y44CB5o+Q5Lqk6IiH5o6o6YCB55uu5YmN5bey5a2Y5Zyo55qE5pS55YuV44CC5a6M5pW05qqi5p+l5Li75YCJ5bqr5Y+K5omA5pyJIHN1Ym1vZHVsZXPvvIjljIXlkKvlt6Lni4Agc3VibW9kdWxlc++8ieeahCBHaXQg54uA5oWL44CB5bey5pqr5a2Y6IiH5pyq5pqr5a2Y5beu55Ww5Y+K5pyq6L+96Lmk5qqU5qGI44CC6L6o6K2Y5Lim5o6S6Zmk5omA5pyJ57eo6K2v44CB5bu6572u44CB5ris6Kmm44CB5b+r5Y+W44CB6KiY6YyE44CB5aWX5Lu25oiW5bel5YW355Si55Sf55qE5pqr5a2Y5qqU6IiH55Si54mp77yM5LiN5b6X5o+Q5Lqk6YCZ5Lqb5qqU5qGI77yM5Lmf5LiN5b6X54K65q2k5L+u5pS5IC5naXRpZ25vcmXjgILlsIflhbbppJjmlLnli5Xkvp3lip/og73oiIfpl5zoga/mgKfliIbpoZ7vvIzliIbmiJDlpJrnrYbnr4TlnI3muIXmpZrnmoQgY29tbWl077yb5LiN5Y+v5oqK5LiN55u46Zec5Yqf6IO95re35Zyo5ZCM5LiA562G44CC5q+P562GIGNvbW1pdCDlv4XpoIjkvb/nlKjkuK3mlocgc3ViamVjdO+8jOS4puWMheWQq+ips+e0sOS4reaWhyBib2R577yM6Kqq5piO6K6K5pu05YWn5a6544CB55uu55qE5Y+K6YeN6KaB5b2x6Z+/44CCc3VibW9kdWxlIOWFp+WmguacieespuWQiOaineS7tueahOaUueWLle+8jOWFiOWcqOipsiBzdWJtb2R1bGUg55qE55uu5YmN5YiG5pSv5YiG5om5IGNvbW1pdCDkuKYgcHVzaO+8jOWGjeaWvOS4u+WAieW6q+aPkOS6pOabtOaWsOW+jOeahCBzdWJtb2R1bGUg5oyH5qiZ77yb5L6d55u45L6d6aCG5bqP5o6o6YCB5omA5pyJ5Y+X5b2x6Z+/5YCJ5bqr55qE55uu5YmN5YiG5pSv44CC5o+Q5Lqk5YmN5YaN5qyh5qqi5p+l5ZCE57WEIGRpZmbvvIzpgb/lhY3mlY/mhJ/os4fmlpnjgIHmmqvlrZjmqpTmiJbkuI3lsazmlrzoqbLlip/og73nmoTlhaflrrnooqvntI3lhaXjgILoi6XmspLmnInlj6/mj5DkuqTmlLnli5XvvIznm7TmjqXmmI7norrlm57loLHjgILoi6XnvLrlsJHpgaDnq6/jgIF1cHN0cmVhbeOAgeasiumZkOaIlumBh+WIsOeEoeazleWuieWFqOWIpOaWt+eahOaDheazge+8jOS4jeW+l+aTheiHquS/ruaUueaqlOahiOaIluatt+WPsu+8jOaHieWBnOatouebuOmXnOaTjeS9nOS4pua4healmuiqquaYjuOAguaOqOmAgeWJjeW/hemgiOWFiOaqouafpSByZW1vdGXvvJvlj6rog73mjqjpgIHliLDnm67liY3luLPomZ/lj6/lr6vlhaXnmoQgZm9ya++8iOmAmuW4uOaYryBvcmlnaW7vvInvvIzkuI3lvpflmJfoqabmjqjpgIHllK/oroAgdXBzdHJlYW3jgILljbPkvb/nm67liY3liIbmlK/ov73ouaQgdXBzdHJlYW3vvIzkuZ/opoHlsIcgSEVBRCDmjqjpgIHliLAgb3JpZ2luIOeahOWQjOWQjeWIhuaUr+OAgiDlhajpg6jlrozmiJDlvozliJflh7rlu7rnq4vnmoQgY29tbWl0c+OAgeacquaPkOS6pOmgheebruWPiuWQhCBwdXNoIOe1kOaenOOAgg=="
set "CODEX_PROMPT_SUFFIX_B64=IOS4u+WAieW6q+W/hemgiOWcqOaJgOacieaPkOS6pOWujOaIkOW+jO+8jOS7pSBnaXQgcHVzaCBvcmlnaW4gSEVBRDptYXN0ZXIg5piO56K65o6o6YCB5YiwIG9yaWdpbi9tYXN0ZXLjgII="

where codex >nul 2>&1
if errorlevel 1 (
    echo Codex CLI was not found in PATH.
    exit /b 9009
)

set "CODEX_PROMPT_FILE=%TEMP%\superexplorer-codex-prompt-%RANDOM%-%RANDOM%.txt"
powershell.exe -NoLogo -NoProfile -NonInteractive -Command "$prompt = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:CODEX_PROMPT_B64)) + [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($env:CODEX_PROMPT_SUFFIX_B64)); [IO.File]::WriteAllText($env:CODEX_PROMPT_FILE, $prompt, [Text.UTF8Encoding]::new($false))"
if errorlevel 1 (
    echo Failed to prepare the Codex prompt.
    if exist "%CODEX_PROMPT_FILE%" del /q "%CODEX_PROMPT_FILE%" >nul 2>&1
    exit /b 1
)

call codex -a never -s danger-full-access -m gpt-5.3-codex-spark -c model_reasoning_effort="low" -C "%~dp0." exec --ignore-user-config --ignore-rules - < "%CODEX_PROMPT_FILE%"
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
