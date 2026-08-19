!ifndef SUPERDESKTOP_FILES_NSH
!define SUPERDESKTOP_FILES_NSH

!ifndef SD_APP_EXE
    !error "SD_APP_EXE must be provided by build_install.lua"
!endif
!ifndef SD_GUARDIAN_EXE
    !error "SD_GUARDIAN_EXE must be provided by build_install.lua"
!endif
!ifndef SD_INSTALLER_EXE
    !error "SD_INSTALLER_EXE must be provided by build_install.lua"
!endif
!ifndef SD_PROVIDER_EXE
    !error "SD_PROVIDER_EXE must be provided by build_install.lua"
!endif
!ifndef SD_NOTIFICATION_EXE
    !error "SD_NOTIFICATION_EXE must be provided by build_install.lua"
!endif
!ifndef SD_STATUS_EXE
    !error "SD_STATUS_EXE must be provided by build_install.lua"
!endif
!ifndef SD_TASKBAR_STATE_EXE
    !error "SD_TASKBAR_STATE_EXE must be provided by build_install.lua"
!endif

!define SUPERDESKTOP_PRODUCT_KEY "Software\SuperDesktop"
!define SUPERDESKTOP_UNINSTALL_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\SuperDesktop"

!macro QuiesceSuperDesktopFiles TARGET
    InitPluginsDir
    SetOutPath "$PLUGINSDIR"
    File /oname=superdesktop-process-closer.exe "${SD_INSTALLER_EXE}"
    nsExec::ExecToStack '"$PLUGINSDIR\superdesktop-process-closer.exe" quiesce --install-dir "${TARGET}"'
    Pop $0
    Pop $1
    ${If} $0 != 0
        DetailPrint "Unable to close running SuperExplorer/SuperDesktop processes: exit=$0 $1"
        MessageBox MB_ICONSTOP|MB_OK "無法自動關閉執行中的 SuperExplorer 或 SuperDesktop：$1"
        Abort
    ${EndIf}
    DetailPrint "Running SuperExplorer/SuperDesktop processes closed and verified: $1"
!macroend

!macro InstallSuperDesktopFiles TARGET
    SetOutPath "${TARGET}"
    SetOverwrite on
    File /oname=superdesktop-app.exe "${SD_APP_EXE}"
    File /oname=superdesktop-guardian.exe "${SD_GUARDIAN_EXE}"
    File /oname=shell-installer.exe "${SD_INSTALLER_EXE}"
    File /oname=shell-provider-host.exe "${SD_PROVIDER_EXE}"
    File /oname=notification-area-host.exe "${SD_NOTIFICATION_EXE}"
    File /oname=system-status-host.exe "${SD_STATUS_EXE}"
    File /oname=taskbar-state-host.exe "${SD_TASKBAR_STATE_EXE}"

    CreateDirectory "$SMPROGRAMS\SuperDesktop"
    CreateShortcut "$SMPROGRAMS\SuperDesktop\SuperDesktop.lnk" "${TARGET}\superdesktop-app.exe" "--shell"
    CreateShortcut "$DESKTOP\SuperDesktop.lnk" "${TARGET}\superdesktop-app.exe" "--shell"

    WriteRegStr HKLM "${SUPERDESKTOP_PRODUCT_KEY}" "InstallDir" "${TARGET}"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "DisplayName" "SuperDesktop"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "DisplayVersion" "${APP_VERSION}"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "Publisher" "Damody"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "URLInfoAbout" "https://github.com/damody/SuperDesktop"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "InstallLocation" "${TARGET}"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "DisplayIcon" "${TARGET}\superdesktop-app.exe"
    WriteRegStr HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "UninstallString" "$\"${TARGET}\Uninstall.exe$\""
    WriteRegDWORD HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "NoModify" 1
    WriteRegDWORD HKLM "${SUPERDESKTOP_UNINSTALL_KEY}" "NoRepair" 1
!macroend

!macro UninstallSuperDesktopFiles TARGET
    Delete "$DESKTOP\SuperDesktop.lnk"
    Delete "$SMPROGRAMS\SuperDesktop\SuperDesktop.lnk"
    RMDir "$SMPROGRAMS\SuperDesktop"
    Delete "${TARGET}\superdesktop-app.exe"
    Delete "${TARGET}\superdesktop-guardian.exe"
    Delete "${TARGET}\shell-installer.exe"
    Delete "${TARGET}\shell-provider-host.exe"
    Delete "${TARGET}\notification-area-host.exe"
    Delete "${TARGET}\system-status-host.exe"
    Delete "${TARGET}\taskbar-state-host.exe"
    DeleteRegKey HKLM "${SUPERDESKTOP_UNINSTALL_KEY}"
    DeleteRegKey HKLM "${SUPERDESKTOP_PRODUCT_KEY}"
!macroend

!endif
