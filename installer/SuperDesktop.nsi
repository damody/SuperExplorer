Unicode True

!include "MUI2.nsh"

!ifdef GENERATED_DEFINES
    !include "${GENERATED_DEFINES}"
!endif

!ifndef APP_VERSION
    !error "APP_VERSION must be provided by build_install.lua"
!endif
!ifndef OUTPUT_FILE
    !error "OUTPUT_FILE must be provided by build_install.lua"
!endif

!include "SuperDesktopFiles.nsh"

!define PRODUCT_NAME "SuperDesktop"
!define PRODUCT_PUBLISHER "Damody"

Name "${PRODUCT_NAME}"
OutFile "${OUTPUT_FILE}"
InstallDir "$PROGRAMFILES64\${PRODUCT_NAME}"
InstallDirRegKey HKLM "${SUPERDESKTOP_PRODUCT_KEY}" "InstallDir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma
SetCompressorDictSize 32
ManifestDPIAware true

VIProductVersion "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "ProductName" "${PRODUCT_NAME}"
VIAddVersionKey /LANG=1033 "FileDescription" "${PRODUCT_NAME} test installer"
VIAddVersionKey /LANG=1033 "CompanyName" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "${PRODUCT_PUBLISHER}"
VIAddVersionKey /LANG=1033 "FileVersion" "${APP_VERSION}"
VIAddVersionKey /LANG=1033 "ProductVersion" "${APP_VERSION}"

!define MUI_ABORTWARNING
!define MUI_FINISHPAGE_RUN "$INSTDIR\superdesktop-app.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Run SuperDesktop in preview mode"
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

Section "SuperDesktop" SEC_MAIN
    SetShellVarContext current
    !insertmacro InstallSuperDesktopFiles "$INSTDIR"
    WriteUninstaller "$INSTDIR\Uninstall.exe"
SectionEnd

Section "Uninstall"
    SetShellVarContext current
    !insertmacro UninstallSuperDesktopFiles "$INSTDIR"
    Delete "$INSTDIR\Uninstall.exe"
    RMDir "$INSTDIR"
SectionEnd
