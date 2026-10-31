; Windows 7 installer: bundles WebView2 109 (the last version that runs on Windows 7/8/8.1),
; so it works without internet. Needs 64-bit Windows 7 or newer (32-bit: installer-hooks-win7-x86.nsh).
!include WinVer.nsh
!include x64.nsh

!define MUI_CUSTOMFUNCTION_GUIINIT HpclCheckWindowsVersion

Function HpclCheckWindowsVersion
  ${IfNot} ${AtLeastBuild} 7600
    MessageBox MB_ICONSTOP|MB_OK "HPCL Operator needs Windows 7 or newer."
    Quit
  ${EndIf}
  ${IfNot} ${RunningX64}
    MessageBox MB_ICONSTOP|MB_OK "This installer is for 64-bit Windows, and this PC runs 32-bit Windows.$\r$\n$\r$\nDownload the installer whose name ends in $\"-win7-x86-setup.exe$\" from the same download page."
    Quit
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
!macroend
