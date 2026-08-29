# Context-menu replacement session

Command:

`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/smoke_context_menu_replacement.ps1 -Profile debug -OutputDirectory build/context-menu-final-replacement-scroll-margin -SkipBuild`

The headful run completed ten item-to-item right-click replacements. Every cycle destroyed the
original popup, transported one validated physical point through the broker protocol, and created
a new popup whose command applied to the replacement target. The app-side replay rejected stale,
foreign-process, and non-owner-window targets before calling `SendInput`.

The retained report also confirms background dismissal, multi-selection preservation, no replay
for clicks inside the popup, outside-left dismissal, Escape dismissal, one broker, bounded
resources, unique popup session identity, and a maximum responsiveness probe of 11 ms.

`cargo test -p explorer-shell-win application_deactivation_dismisses_without_selection_or_replay
-- --test-threads=1` also passed. That controlled lifetime test sends the Win32 application
deactivation message and proves the popup returns cancellation without a command or replay point.

- Report: `build/context-menu-final-replacement-scroll-margin/report.json`
- Screenshot: `build/context-menu-final-replacement-scroll-margin/context-menu-replacement.png`
- Report SHA-256: `4B6741E270165FDD99165FE2E2ACF35F1A2C4F82A8F4674EE93071428B821A3B`
- Screenshot SHA-256: `EA6B6455C520AE26998E41EC6269B2277878510771495777AC287AB286E5F396`
