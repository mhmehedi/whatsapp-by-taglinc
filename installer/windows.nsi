; NSIS script for the Windows installer (built by .github/workflows/windows.yml).
; makensis /DAPP_VERSION=0.1.4 installer\windows.nsi
Unicode true
SetCompressor /SOLID lzma
RequestExecutionLevel user

!define APP "Whatsapp by TagLinc"
!define EXE "${APP}.exe"
!ifndef APP_VERSION
  !define APP_VERSION "0.0.0"
!endif
!define UNINST_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\WhatsappByTagLinc"

Name "${APP}"
OutFile "..\dist\Whatsapp-by-TagLinc-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\${APP}"
VIProductVersion "${APP_VERSION}.0"
VIAddVersionKey "ProductName" "${APP}"
VIAddVersionKey "CompanyName" "TagLinc"
VIAddVersionKey "LegalCopyright" "Copyright (c) 2026 TagLinc"
VIAddVersionKey "FileDescription" "${APP} Setup"
VIAddVersionKey "FileVersion" "${APP_VERSION}"
VIAddVersionKey "ProductVersion" "${APP_VERSION}"

!include "MUI2.nsh"
!define MUI_ICON "..\assets\AppIcon.ico"
!define MUI_UNICON "..\assets\AppIcon.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Open ${APP}"
; The "readme" checkbox is reused as "create desktop shortcut".
!define MUI_FINISHPAGE_SHOWREADME ""
!define MUI_FINISHPAGE_SHOWREADME_TEXT "Create a desktop shortcut"
!define MUI_FINISHPAGE_SHOWREADME_FUNCTION DesktopShortcut
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function DesktopShortcut
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\${EXE}"
FunctionEnd

Section "Install"
  ; Ask a running copy to close so its file can be replaced.
  nsExec::Exec 'taskkill /IM "${EXE}"'
  Sleep 1500
  SetOutPath "$INSTDIR"
  File "/oname=${EXE}" "..\target\x86_64-pc-windows-msvc\release\multi-whatsapp.exe"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "${UNINST_KEY}" "Publisher" "TagLinc"
  WriteRegStr HKCU "${UNINST_KEY}" "DisplayIcon" "$INSTDIR\${EXE}"
  WriteRegStr HKCU "${UNINST_KEY}" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST_KEY}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  ; Logins and account data in %LOCALAPPDATA%\WhatsappByTagLinc are kept.
  nsExec::Exec 'taskkill /IM "${EXE}"'
  Sleep 1500
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\${APP}.lnk"
  Delete "$DESKTOP\${APP}.lnk"
  DeleteRegKey HKCU "${UNINST_KEY}"
  DeleteRegKey HKCU "Software\Classes\AppUserModelId\TagLinc.WhatsappByTagLinc"
SectionEnd
