## Why

`build_install.bat` 目前完成 NSIS 打包後只顯示成功訊息並等待按鍵，使用者還必須自行找到正確版本的 installer。建置流程已掌握並驗證唯一輸出路徑，應在成功後直接交接到該安裝程式，同時避免失敗或檢查模式誤開舊產物。

## What Changes

- 正常與 `--skip-build` 模式在本次 installer 通過既有 PE 驗證後，直接啟動該確切輸出一次。
- `--check`、build/NSIS/validation failure 與 launch failure 均不啟動其他 installer；launch failure 回傳非零結果。
- 新增安全、Unicode-aware、非等待式的 process launch helper，使用分離的 executable 與 working directory。
- `build_install.bat` 移除無條件 pause，保留 Lua exit code，並提供符合實際 launch 結果的繁體中文訊息。
- 加入不會真的進入安裝精靈的 Lua/BAT contract tests 與受控 launch smoke。

## Capabilities

### New Capabilities

- `installer-build-handoff`: 定義成功打包後精確、安全且非阻塞地啟動本次 SuperExplorer installer，以及所有禁止啟動與錯誤傳播條件。

### Modified Capabilities

<!-- 無既有 canonical capability 的 requirement 需要修改。 -->

## Impact

影響根目錄 `build_install.bat`、`build/build_install.lua`、`build/lib/process.lua`、installer build tests 與 UITEST manifest。不改變 NSIS 安裝內容、版本演算法、clean-tree policy、UAC 策略或安裝程式本身的互動流程。
