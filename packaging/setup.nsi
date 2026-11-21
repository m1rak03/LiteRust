Unicode true
!include "MUI2.nsh"
!include "x64.nsh"
Name "LiteRust"
OutFile "LiteRust-0.18.0-alpha.3-Setup-x64.exe"
InstallDir "$LOCALAPPDATA\Programs\LiteRust"
RequestExecutionLevel user
SetCompressor /SOLID lzma
Icon "literust.ico"
UninstallIcon "literust.ico"
InstallDirRegKey HKCU "Software\LiteRust" "InstallDir"
!define MUI_ABORTWARNING
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!define MUI_FINISHPAGE_RUN "$INSTDIR\literust-browser.exe"
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "Russian"
Function .onInit
${IfNot} ${RunningX64}
MessageBox MB_ICONSTOP "Требуется Windows x64."
Abort
${EndIf}
SetShellVarContext current
FunctionEnd
Section "LiteRust"
InitPluginsDir
SetOutPath "$PLUGINSDIR"
File "close-browser.exe"
nsExec::ExecToStack '"$PLUGINSDIR\close-browser.exe" "$INSTDIR" "$LOCALAPPDATA\LiteRustBrowser\diagnostics\install-close.log"'
Pop $0
Pop $1
CreateDirectory "$LOCALAPPDATA\LiteRustBrowser\diagnostics"
FileOpen $4 "$LOCALAPPDATA\LiteRustBrowser\diagnostics\install-helper.log" w
FileWrite $4 "$0$\r$\n$1"
FileClose $4
${If} $0 != 0
MessageBox MB_ICONSTOP "Закройте LiteRust и повторите установку. Открытый браузер не был завершён принудительно." /SD IDOK
SetErrorLevel 1
Abort
${EndIf}
SetOutPath "$INSTDIR"
File "..\target\release\literust-browser.exe"
File "WebView2Loader.dll"
File "..\vendor\xray\xray.exe"
File "..\vendor\sing-box\sing-box.exe"
File "..\vendor\sing-box\libcronet.dll"
File /oname=SINGBOX-LICENSE.txt "..\vendor\sing-box\LICENSE"
File /oname=SINGBOX-SOURCE.txt "..\vendor\sing-box\SOURCE.txt"
File /oname=SINGBOX-COPYING.txt "..\vendor\sing-box\COPYING"
File "system-vpn.ps1"
File /oname=XRAY-LICENSE.txt "..\vendor\xray\LICENSE"
ExecWait '"$INSTDIR\literust-browser.exe" --check-runtime' $2
${If} $2 != 0
InitPluginsDir
SetOutPath "$PLUGINSDIR"
File /oname=WebView2Setup.exe "..\runtime-installer.exe"
ExecWait '"$PLUGINSDIR\WebView2Setup.exe" /silent /install' $0
SetOutPath "$INSTDIR"
ExecWait '"$INSTDIR\literust-browser.exe" --check-runtime' $2
${If} $2 != 0
MessageBox MB_ICONSTOP "WebView2 не установлен. Установка не завершена. Код: $0" /SD IDOK
SetErrorLevel 1
Abort
${EndIf}
${EndIf}
File "literust.ico"
StrCpy $3 "$INSTDIR\literust.ico"
IfFileExists "$LOCALAPPDATA\LiteRustBrowser\custom.ico" 0 +2
StrCpy $3 "$LOCALAPPDATA\LiteRustBrowser\custom.ico"
CreateDirectory "$SMPROGRAMS\LiteRust"
CreateShortcut "$SMPROGRAMS\LiteRust\LiteRust.lnk" "$INSTDIR\literust-browser.exe" "" "$3"
CreateShortcut "$DESKTOP\LiteRust.lnk" "$INSTDIR\literust-browser.exe" "" "$3"
WriteUninstaller "$INSTDIR\Uninstall.exe"
WriteRegStr HKCU "Software\LiteRust" "InstallDir" "$INSTDIR"
WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiteRust" "DisplayVersion" "0.18.0-alpha.3"
WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiteRust" "DisplayName" "LiteRust"
WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiteRust" "UninstallString" '"$INSTDIR\Uninstall.exe"'
WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiteRust" "DisplayIcon" "$INSTDIR\literust-browser.exe"
SectionEnd
Section "Uninstall"
SetShellVarContext current
Delete "$INSTDIR\literust.ico"
DeleteRegKey HKCU "Software\LiteRust"
Delete "$DESKTOP\LiteRust.lnk"
Delete "$SMPROGRAMS\LiteRust\LiteRust.lnk"
RMDir "$SMPROGRAMS\LiteRust"
Delete "$INSTDIR\literust-browser.exe"
Delete "$INSTDIR\WebView2Loader.dll"
Delete "$INSTDIR\xray.exe"
Delete "$INSTDIR\sing-box.exe"
Delete "$INSTDIR\libcronet.dll"
Delete "$INSTDIR\system-vpn.ps1"
Delete "$INSTDIR\system-vpn.ps1.launcher.log"
Delete "$INSTDIR\system-vpn.ps1.wrapper.log"
Delete "$INSTDIR\system-vpn.ps1.fatal.log"
Delete "$INSTDIR\system-vpn.ps1.stderr.log"
Delete "$INSTDIR\tun-helper-error.log"
Delete "$INSTDIR\SINGBOX-LICENSE.txt"
Delete "$INSTDIR\SINGBOX-SOURCE.txt"
Delete "$INSTDIR\SINGBOX-COPYING.txt"
Delete "$INSTDIR\XRAY-LICENSE.txt"
Delete "$INSTDIR\Uninstall.exe"
RMDir "$INSTDIR"
DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\LiteRust"
; Browser profile is intentionally preserved.
SectionEnd
