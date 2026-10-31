; Windows 10/11 installer. On Windows 7 / 8 / 8.1 it stops with a clear message pointing to the
; separate "win7" installer. Without this, Microsoft's WebView2 setup (launched by this installer)
; fails there with "PackageIdFromFullName could not be located in KERNEL32.dll".
!include WinVer.nsh

!define MUI_CUSTOMFUNCTION_GUIINIT HpclCheckWindowsVersion

Function HpclCheckWindowsVersion
  ${IfNot} ${AtLeastBuild} 10240
    MessageBox MB_ICONSTOP|MB_OK "This installer is for Windows 10 and Windows 11.$\r$\n$\r$\nThis PC is running an older Windows (7, 8 or 8.1), which Microsoft no longer supports and which can't run this installer's screen component (WebView2).$\r$\n$\r$\nFor Windows 7, 8 and 8.1, download the installer whose name ends in $\"-win7-x64-setup.exe$\" from the same download page."
    Quit
  ${EndIf}
FunctionEnd

!macro NSIS_HOOK_PREINSTALL
!macroend
