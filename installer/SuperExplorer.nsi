Unicode True

!include "MUI2.nsh"

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
!ifndef WORKER_EXE
    !error "WORKER_EXE must be provided by build_install.lua"
!endif
!ifndef EVERYTHING_DLL
    !error "EVERYTHING_DLL must be provided by build_install.lua"
!endif
!ifndef PLUGIN_DLL
    !error "PLUGIN_DLL must be provided by build_install.lua"
!endif

!define PRODUCT_NAME "SuperExplorer"
!define PRODUCT_PUBLISHER "Damody"
!define PRODUCT_URL "https://github.com/damody/SuperExplorer"
!define PRODUCT_REG_KEY "Software\SuperExplorer"
!define PRODUCT_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SuperExplorer"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$LOCALAPPDATA\Programs\${PRODUCT_NAME}"
InstallDirRegKey HKCU "${PRODUCT_REG_KEY}" "InstallDir"
RequestExecutionLevel user
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
!define MUI_FINISHPAGE_RUN "$INSTDIR\SuperExplorer.exe"
!define MUI_FINISHPAGE_RUN_TEXT "執行 SuperExplorer"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--plugin-dll $\"$INSTDIR\plugins\rust_folder_size_visual_column.dll$\""
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
    SetOutPath "$INSTDIR"
    SetOverwrite on

    File "${APP_EXE}"
    File /oname=explorer-extension-broker.exe "${BROKER_EXE}"
    File /oname=explorer-extension-worker.exe "${WORKER_EXE}"
    File /oname=Everything64.dll "${EVERYTHING_DLL}"

    SetOutPath "$INSTDIR\plugins"
    File /oname=rust_folder_size_visual_column.dll "${PLUGIN_DLL}"

    SetOutPath "$INSTDIR"
    WriteUninstaller "$INSTDIR\Uninstall.exe"

    CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
    CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\SuperExplorer.exe" "--plugin-dll $\"$INSTDIR\plugins\rust_folder_size_visual_column.dll$\""
    CreateShortcut "$DESKTOP\${PRODUCT_NAME}.lnk" "$INSTDIR\SuperExplorer.exe" "--plugin-dll $\"$INSTDIR\plugins\rust_folder_size_visual_column.dll$\""

    WriteRegStr HKCU "${PRODUCT_REG_KEY}" "InstallDir" "$INSTDIR"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "DisplayName" "${PRODUCT_NAME}"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "Publisher" "${PRODUCT_PUBLISHER}"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "URLInfoAbout" "${PRODUCT_URL}"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "DisplayIcon" "$INSTDIR\SuperExplorer.exe"
    WriteRegStr HKCU "${PRODUCT_UNINSTALL_KEY}" "UninstallString" "$\"$INSTDIR\Uninstall.exe$\""
    WriteRegDWORD HKCU "${PRODUCT_UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKCU "${PRODUCT_UNINSTALL_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
    SetShellVarContext current

    Delete "$DESKTOP\${PRODUCT_NAME}.lnk"
    Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
    RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

    Delete "$INSTDIR\SuperExplorer.exe"
    Delete "$INSTDIR\explorer-extension-broker.exe"
    Delete "$INSTDIR\explorer-extension-worker.exe"
    Delete "$INSTDIR\Everything64.dll"
    Delete "$INSTDIR\plugins\rust_folder_size_visual_column.dll"
    RMDir "$INSTDIR\plugins"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"

    DeleteRegKey HKCU "${PRODUCT_UNINSTALL_KEY}"
    DeleteRegKey HKCU "${PRODUCT_REG_KEY}"
SectionEnd
