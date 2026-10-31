; 32-bit Windows 7 installer: bundles the 32-bit WebView2 109 (the last version that runs on
; Windows 7/8/8.1). Also runs on 64-bit Windows, but the x64 installer is the better fit there.
!include WinVer.nsh

!define MUI_CUSTOMFUNCTION_GUIINIT HpclCheckWindowsVersion

Function HpclCheckWindowsVersion
  ${IfNot} ${AtLeastBuild} 7600
    MessageBox MB_ICONSTOP|MB_OK "HPCL Operator needs Windows 7 or newer."
    Quit
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
!macroend
