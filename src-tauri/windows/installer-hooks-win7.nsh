; Windows 7 installer: bundles WebView2 109 (the last version that runs on Windows 7/8/8.1),
; so it works without internet. Needs 64-bit Windows 7 or newer.
!include WinVer.nsh
!include x64.nsh

!define MUI_CUSTOMFUNCTION_GUIINIT HpclCheckWindowsVersion

Function HpclCheckWindowsVersion
  ${IfNot} ${RunningX64}
  ${OrIfNot} ${AtLeastBuild} 7600
    MessageBox MB_ICONSTOP|MB_OK "HPCL Operator needs 64-bit Windows 7 or newer.$\r$\n$\r$\nThis PC is running 32-bit Windows or a version older than Windows 7."
    Quit
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
!macroend
