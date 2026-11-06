Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
!include "WinVer.nsh"

Name "OpenVector"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\OpenVector"
RequestExecutionLevel user
SetCompressor /SOLID lzma
!define MUI_ICON "${APP_ICON}"
!define MUI_UNICON "${APP_ICON}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=1033 "ProductName" "OpenVector"
VIAddVersionKey /LANG=1033 "FileDescription" "OpenVector installer"
VIAddVersionKey /LANG=1033 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=1033 "LegalCopyright" "OpenVector contributors"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

Function .onInit
  ${IfNot} ${RunningX64}
    MessageBox MB_OK|MB_ICONSTOP "OpenVector requires 64-bit Windows."
    Abort
  ${EndIf}
  ${IfNot} ${AtLeastWin10}
    MessageBox MB_OK|MB_ICONSTOP "OpenVector requires Windows 10 or newer."
    Abort
  ${EndIf}
FunctionEnd

Section "OpenVector"
  SetShellVarContext current
  SetRegView 64
  SetOutPath "$INSTDIR"
  File /r "${STAGE}/*"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateDirectory "$SMPROGRAMS\OpenVector"
  CreateShortcut "$SMPROGRAMS\OpenVector\OpenVector.lnk" "$INSTDIR\openvector-gui.exe"
  CreateShortcut "$SMPROGRAMS\OpenVector\Uninstall.lnk" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "DisplayName" "OpenVector"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "Publisher" "OpenVector contributors"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "DisplayIcon" "$INSTDIR\openvector-gui.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector" "NoRepair" 1
SectionEnd

Function un.DeletePackageFile
  Exch $0
  Push $1
retry:
  StrCpy $1 20
attempt:
  ClearErrors
  Delete "$0"
  IfErrors 0 removed
  ; Allow brief file locks without hiding a persistent deletion failure.
  IntOp $1 $1 - 1
  IntCmp $1 0 blocked
  Sleep 100
  Goto attempt
blocked:
  MessageBox MB_RETRYCANCEL|MB_ICONEXCLAMATION "Could not remove this file:$\r$\n$0$\r$\n$\r$\nClose OpenVector or any program using the file, then choose Retry. Choose Cancel to stop uninstalling." /SD IDCANCEL IDRETRY retry
  SetErrorLevel 1
  Quit
removed:
  Pop $1
  Pop $0
FunctionEnd

Section "Uninstall"
  SetShellVarContext current
  SetRegView 64
  ; Generated from the staged package. Never recursively delete the install
  ; directory: user-created files and per-user preferences must survive.
  !include "${UNINSTALL_INCLUDE}"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  Delete "$SMPROGRAMS\OpenVector\OpenVector.lnk"
  Delete "$SMPROGRAMS\OpenVector\Uninstall.lnk"
  RMDir "$SMPROGRAMS\OpenVector"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenVector"
SectionEnd
