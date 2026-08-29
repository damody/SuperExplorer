# Remote context-menu parity status

## ADB current runner

- Target: `adb://emulator-5554/sdcard/Download` background.
- Result: passed; UIA exposed `Remote file context menu` and the `新增資料夾` button.
- Screenshot: `adb-background-light-current-runner.png`.
- SHA-256: `E12FFCB09BA3BCF56EDC57E4A5A340BC2004DD4C46F327E552034BDC32752C28`.
- The isolated profile had no transferable clipboard, so Paste was correctly absent and was
  not treated as a failure.

## SFTP current runner

- A saved profile was copied into an isolated `LOCALAPPDATA` test session; no credential or
  endpoint was logged into this evidence.
- Two runs (3-second and 15-second settle windows) did not produce the remote first viewport,
  so background context commands remained disabled and no menu existed to measure.
- Status: blocked by the current SFTP connectivity/authentication environment, not passed and
  not counted toward G13.

## Matrix status

Only the ADB light/background/current-runner cell has physical evidence. Item/folder, dark,
the approved DPI matrix, SFTP, interaction, keyboard, and accessibility cells remain open.
