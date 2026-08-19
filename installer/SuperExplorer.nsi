Unicode True

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "StrFunc.nsh"
${Using:StrFunc} StrStr
${Using:StrFunc} UnStrStr

!ifdef GENERATED_DEFINES
    !include "${GENERATED_DEFINES}"
!endif

!ifndef APP_VERSION
    !error "APP_VERSION must be provided by build_install.lua"
!endif
!ifndef APP_EXE
    !error "APP_EXE must be provided by build_install.lua"
!endif
!ifndef OUTPUT_FILE
    !error "OUTPUT_FILE must be provided by build_install.lua"
!endif
!ifndef BROKER_EXE
    !error "BROKER_EXE must be provided by build_install.lua"
!endif
!ifndef MFT_HELPER_EXE
    !error "MFT_HELPER_EXE must be provided by build_install.lua"
!endif
!ifndef MFT_SERVICE_EXE
    !error "MFT_SERVICE_EXE must be provided by build_install.lua"
!endif
!ifndef WORKER_EXE
    !error "WORKER_EXE must be provided by build_install.lua"
!endif
!ifndef EVERYTHING_DLL
    !error "EVERYTHING_DLL must be provided by build_install.lua"
!endif
!ifndef PLUGIN_FOLDER_SIZE
    !error "All eight bundled plugin paths must be provided by build_install.lua"
!endif

!ifdef INCLUDE_SUPERDESKTOP
    !include "SuperDesktopFiles.nsh"
!endif

!define PRODUCT_NAME "SuperExplorer"
!define PRODUCT_PUBLISHER "Damody"
!define PRODUCT_URL "https://github.com/damody/SuperExplorer"
!define PRODUCT_REG_KEY "Software\SuperExplorer"
!define PRODUCT_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SuperExplorer"

!macro ExecServiceChecked COMMAND FAILURE_TEXT
    nsExec::ExecToStack '${COMMAND}'
    Pop $0
    Pop $1
    ${If} $0 != 0
        DetailPrint "${FAILURE_TEXT}: exit=$0 $1"
        MessageBox MB_ICONSTOP|MB_OK "${FAILURE_TEXT}$\r$\n$\r$\n$1"
        Abort
    ${EndIf}
!macroend

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
InstallDirRegKey HKLM "${PRODUCT_REG_KEY}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
SetCompressorDictSize 32
ManifestDPIAware true

VIProductVersion "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${PRODUCT_NAME} 安裝程式"
VIAddVersionKey /LANG=1033 "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=1033 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${APP_VERSION}"

!define MUI_ABORTWARNING
!define PLUGIN_ARGS "--plugin-dll $\"$INSTDIR\plugins\rust_folder_size_visual_column.dll$\" --plugin-dll $\"$INSTDIR\plugins\rust_folder_size_map_view.dll$\" --plugin-dll $\"$INSTDIR\plugins\rust_tokei_code_lines_column.dll$\" --plugin-dll $\"$INSTDIR\plugins\lua_tokei_code_lines_column.dll$\" --plugin-dll $\"$INSTDIR\plugins\rust_lock_owner_column.dll$\" --plugin-dll $\"$INSTDIR\plugins\rust_exif_rename_command.dll$\" --plugin-dll $\"$INSTDIR\plugins\rust_7z_virtual_folder.dll$\" --plugin-dll $\"$INSTDIR\plugins\lua_bulk_folder_generator.dll$\""
!ifdef INCLUDE_SUPERDESKTOP
    !define MUI_FINISHPAGE_RUN "$INSTDIR\superdesktop-app.exe"
    !define MUI_FINISHPAGE_RUN_TEXT "執行 SuperDesktop"
    !define MUI_FINISHPAGE_RUN_PARAMETERS "--shell"
!else
    !define MUI_FINISHPAGE_RUN "$INSTDIR\SuperExplorer.exe"
    !define MUI_FINISHPAGE_RUN_TEXT "執行 SuperExplorer"
    !define MUI_FINISHPAGE_RUN_PARAMETERS "${PLUGIN_ARGS}"
!endif
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_UNPAGE_FINISH

!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "TradChinese"
!insertmacro MUI_LANGUAGE "SimpChinese"

Section "SuperExplorer" SEC_MAIN
    SetShellVarContext current

    ; Never overwrite the service binary until SCM confirms a full stop.
    ; Error 1060 is the expected first-install case; other failures are fatal.
    DetailPrint "Waiting for SuperExplorer MFT Windows Service to stop before upgrade."
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        ${StrStr} $3 $1 "1060"
        StrCmp $3 "" service_query_before_install_failed service_ready_for_files
    ${EndIf}

    ${StrStr} $3 $1 "STOPPED"
    StrCmp $3 "" service_check_stop_pending service_ready_for_files

service_check_stop_pending:
    ${StrStr} $3 $1 "STOP_PENDING"
    StrCmp $3 "" service_request_stop service_wait_stopped_init

service_query_before_install_failed:
    DetailPrint "Unable to query SuperExplorer MFT Windows Service before upgrade: exit=$0 $1"
    MessageBox MB_ICONSTOP|MB_OK "無法在更新前查詢 SuperExplorer MFT Windows Service 狀態。$\r$\n$\r$\n$1"
    Abort

service_request_stop:
    nsExec::ExecToStack '"$SYSDIR\sc.exe" stop SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        ; Error 1062 means the service reached STOPPED between query and stop.
        ${StrStr} $3 $1 "1062"
        StrCmp $3 "" service_stop_failed service_wait_stopped_init
    ${EndIf}

service_wait_stopped_init:
    StrCpy $2 0
service_wait_stopped:
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        Goto service_stop_query_failed
    ${EndIf}
    ${StrStr} $3 $1 "STOPPED"
    StrCmp $3 "" service_not_stopped service_ready_for_files

service_not_stopped:
    IntOp $2 $2 + 1
    IntCmp $2 30 service_stop_timeout service_stop_retry service_stop_timeout
service_stop_retry:
    Sleep 500
    Goto service_wait_stopped

service_stop_failed:
    DetailPrint "Unable to stop SuperExplorer MFT Windows Service before upgrade: exit=$0 $1"
    MessageBox MB_ICONSTOP|MB_OK "無法在更新前停止 SuperExplorer MFT Windows Service。$\r$\n$\r$\n$1"
    Abort

service_stop_query_failed:
    DetailPrint "Unable to query stopping SuperExplorer MFT Windows Service: exit=$0 $1"
    MessageBox MB_ICONSTOP|MB_OK "停止服務時無法查詢 SuperExplorer MFT Windows Service 狀態。$\r$\n$\r$\n$1"
    Abort

service_stop_timeout:
    MessageBox MB_ICONSTOP|MB_OK "SuperExplorer MFT Windows Service 未能在 15 秒內進入 STOPPED 狀態。安裝尚未覆蓋服務檔案。"
    Abort

service_ready_for_files:
    DetailPrint "SuperExplorer MFT Windows Service is absent or STOPPED; installing files."
    SetOutPath "$INSTDIR"
    SetOverwrite on

    File "${APP_EXE}"
    File /oname=explorer-extension-broker.exe "${BROKER_EXE}"
    File /oname=superexplorer-mft-helper.exe "${MFT_HELPER_EXE}"
    File /oname=superexplorer-mft-service.exe "${MFT_SERVICE_EXE}"
    File /oname=explorer-extension-worker.exe "${WORKER_EXE}"
    File /oname=Everything64.dll "${EVERYTHING_DLL}"

    SetOutPath "$INSTDIR\plugins"
    File /oname=rust_folder_size_visual_column.dll "${PLUGIN_FOLDER_SIZE}"
    File /oname=rust_folder_size_map_view.dll "${PLUGIN_SIZE_MAP}"
    File /oname=rust_tokei_code_lines_column.dll "${PLUGIN_RUST_TOKEI}"
    File /oname=lua_tokei_code_lines_column.dll "${PLUGIN_LUA_TOKEI}"
    File /oname=rust_lock_owner_column.dll "${PLUGIN_LOCK_OWNER}"
    File /oname=rust_exif_rename_command.dll "${PLUGIN_EXIF_RENAME}"
    File /oname=rust_7z_virtual_folder.dll "${PLUGIN_7Z}"
    File /oname=lua_bulk_folder_generator.dll "${PLUGIN_BULK_FOLDER}"

    !ifdef INCLUDE_SUPERDESKTOP
        !insertmacro InstallSuperDesktopFiles "$INSTDIR"
    !endif

    SetOutPath "$INSTDIR"
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        !insertmacro ExecServiceChecked '"$SYSDIR\sc.exe" create SuperExplorerMft binPath= $\"$INSTDIR\superexplorer-mft-service.exe$\" start= auto obj= LocalSystem DisplayName= "SuperExplorer MFT Service"' "無法建立 SuperExplorer MFT Windows Service"
    ${Else}
        !insertmacro ExecServiceChecked '"$SYSDIR\sc.exe" config SuperExplorerMft binPath= $\"$INSTDIR\superexplorer-mft-service.exe$\" start= auto obj= LocalSystem DisplayName= "SuperExplorer MFT Service"' "無法設定 SuperExplorer MFT Windows Service"
    ${EndIf}
    !insertmacro ExecServiceChecked '"$SYSDIR\sc.exe" description SuperExplorerMft "Read-only NTFS metadata index for SuperExplorer folder snapshots"' "無法設定 SuperExplorer MFT Windows Service 描述"
    !insertmacro ExecServiceChecked '"$SYSDIR\sc.exe" start SuperExplorerMft' "無法啟動 SuperExplorer MFT Windows Service"

    StrCpy $2 0
service_wait_running:
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        MessageBox MB_ICONSTOP|MB_OK "無法查詢 SuperExplorer MFT Windows Service 狀態。$\r$\n$\r$\n$1"
        Abort
    ${EndIf}
    ${StrStr} $3 $1 "RUNNING"
    StrCmp $3 "" service_not_running service_running
service_not_running:
    IntOp $2 $2 + 1
    IntCmp $2 30 service_start_timeout service_retry_wait service_start_timeout
service_retry_wait:
    Sleep 500
    Goto service_wait_running
service_start_timeout:
    MessageBox MB_ICONSTOP|MB_OK "SuperExplorer MFT Windows Service 未能在 15 秒內進入 RUNNING 狀態。"
    Abort
service_running:
    DetailPrint "SuperExplorer MFT Windows Service is RUNNING as LocalSystem."

    WriteUninstaller "$INSTDIR\Uninstall.exe"

    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\SuperExplorer.exe" "${PLUGIN_ARGS}"
    CreateShortcut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\SuperExplorer.exe" "${PLUGIN_ARGS}"

    WriteRegStr HKLM "${PRODUCT_REG_KEY}" "InstallDir" "$INSTDIR"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "URLInfoAbout" "${PRODUCT_URL}"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\SuperExplorer.exe"
    WriteRegStr HKLM "${PRODUCT_UNINSTALL_KEY}" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegDWORD HKLM "${PRODUCT_UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKLM "${PRODUCT_UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
    SetShellVarContext current

    DetailPrint "Waiting for SuperExplorer MFT Windows Service to stop before uninstall."
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        ${UnStrStr} $3 $1 "1060"
        StrCmp $3 "" un.service_query_failed un.service_ready_for_delete
    ${EndIf}
    ${UnStrStr} $3 $1 "STOPPED"
    StrCmp $3 "" un.service_check_stop_pending un.service_ready_for_delete

un.service_check_stop_pending:
    ${UnStrStr} $3 $1 "STOP_PENDING"
    StrCmp $3 "" un.service_request_stop un.service_wait_stopped_init

un.service_request_stop:
    nsExec::ExecToStack '"$SYSDIR\sc.exe" stop SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        ${UnStrStr} $3 $1 "1062"
        StrCmp $3 "" un.service_stop_failed un.service_wait_stopped_init
    ${EndIf}

un.service_wait_stopped_init:
    StrCpy $2 0
un.service_wait_stopped:
    nsExec::ExecToStack '"$SYSDIR\sc.exe" query SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 != 0
        Goto un.service_stop_query_failed
    ${EndIf}
    ${UnStrStr} $3 $1 "STOPPED"
    StrCmp $3 "" un.service_not_stopped un.service_ready_for_delete

un.service_not_stopped:
    IntOp $2 $2 + 1
    IntCmp $2 30 un.service_stop_timeout un.service_stop_retry un.service_stop_timeout
un.service_stop_retry:
    Sleep 500
    Goto un.service_wait_stopped

un.service_query_failed:
    DetailPrint "Unable to query SuperExplorer MFT Windows Service before uninstall: exit=$0 $1"
    MessageBox MB_ICONSTOP|MB_OK "無法在解除安裝前查詢 SuperExplorer MFT Windows Service 狀態。$\r$\n$\r$\n$1"
    Abort

un.service_stop_failed:
    DetailPrint "Unable to stop SuperExplorer MFT Windows Service before uninstall: exit=$0 $1"
    MessageBox MB_ICONSTOP|MB_OK "無法在解除安裝前停止 SuperExplorer MFT Windows Service。$\r$\n$\r$\n$1"
    Abort

un.service_stop_query_failed:
    DetailPrint "Unable to query stopping SuperExplorer MFT Windows Service during uninstall: exit=$0 $1"
    MessageBox MB_ICONSTOP|MB_OK "解除安裝停止服務時無法查詢 SuperExplorer MFT Windows Service 狀態。$\r$\n$\r$\n$1"
    Abort

un.service_stop_timeout:
    MessageBox MB_ICONSTOP|MB_OK "SuperExplorer MFT Windows Service 未能在 15 秒內進入 STOPPED 狀態。解除安裝尚未刪除服務或檔案。"
    Abort

un.service_ready_for_delete:
    DetailPrint "SuperExplorer MFT Windows Service is absent or STOPPED; uninstalling files."
    nsExec::ExecToStack '"$SYSDIR\sc.exe" delete SuperExplorerMft'
    Pop $0
    Pop $1
    ${If} $0 == 0
        DetailPrint "SuperExplorer MFT Windows Service deletion accepted by SCM."
    ${ElseIf} $0 == 1060
        DetailPrint "SuperExplorer MFT Windows Service was already absent."
    ${Else}
        DetailPrint "Unable to delete SuperExplorer MFT Windows Service: exit=$0 $1"
        MessageBox MB_ICONSTOP|MB_OK "無法刪除 SuperExplorer MFT Windows Service；解除安裝尚未刪除服務執行檔。$\r$\n$\r$\n$1"
        Abort
    ${EndIf}

    !ifdef INCLUDE_SUPERDESKTOP
        !insertmacro UninstallSuperDesktopFiles "$INSTDIR"
    !endif

    Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

    Delete "$INSTDIR\SuperExplorer.exe"
    Delete "$INSTDIR\explorer-extension-broker.exe"
    Delete "$INSTDIR\superexplorer-mft-helper.exe"
    Delete "$INSTDIR\superexplorer-mft-service.exe"
    Delete "$INSTDIR\explorer-extension-worker.exe"
    Delete "$INSTDIR\Everything64.dll"
    Delete "$INSTDIR\plugins\rust_folder_size_visual_column.dll"
    Delete "$INSTDIR\plugins\rust_folder_size_map_view.dll"
    Delete "$INSTDIR\plugins\rust_tokei_code_lines_column.dll"
    Delete "$INSTDIR\plugins\lua_tokei_code_lines_column.dll"
    Delete "$INSTDIR\plugins\rust_lock_owner_column.dll"
    Delete "$INSTDIR\plugins\rust_exif_rename_command.dll"
    Delete "$INSTDIR\plugins\rust_7z_virtual_folder.dll"
    Delete "$INSTDIR\plugins\lua_bulk_folder_generator.dll"
    RMDir "$INSTDIR\plugins"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"

    ; MFT durability state is service-owned. Upgrade, repair, and uninstall
    ; deliberately preserve both legacy and SQLite caches so a rollback can
    ; ignore SQLite and rebuild legacy state without stop-time deletion.
    DetailPrint "Preserving service-owned MFT cache for reinstall or rollback."
    DeleteRegKey HKLM "${PRODUCT_UNINSTALL_KEY}"
    DeleteRegKey HKLM "${PRODUCT_REG_KEY}"
SectionEnd
