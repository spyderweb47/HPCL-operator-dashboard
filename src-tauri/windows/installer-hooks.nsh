; Stop on Windows 7 / 8 / 8.1 with a clear message. Without this, Microsoft's WebView2 setup
; (launched by the installer) fails there with "PackageIdFromFullName could not be located in
; KERNEL32.dll". The app itself also needs Windows 10 or newer.
!include WinVer.nsh

!define MUI_CUSTOMFUNCTION_GUIINIT HpclCheckWindowsVersion

Function HpclCheckWindowsVersion
  ${IfNot} ${AtLeastBuild} 10240
    MessageBox MB_ICONSTOP|MB_OK "HPCL Operator needs Windows 10 or Windows 11 (64-bit).$\r$\n$\r$\nThis PC is running an older Windows (7, 8 or 8.1), which Microsoft no longer supports and which can't run the app's screen component (WebView2).$\r$\n$\r$\nPlease install it on a Windows 10/11 PC."
    Quit
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
!macroend
