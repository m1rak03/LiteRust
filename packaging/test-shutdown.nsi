Unicode true
Name "LiteRust shutdown regression"
OutFile "test-shutdown.exe"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\LiteRust"
Section
InitPluginsDir
SetOutPath "$PLUGINSDIR"
File "close-browser.ps1"
nsExec::ExecToStack '"$WINDIR\Sysnative\WindowsPowerShell\v1.0\powershell.exe" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\close-browser.ps1" -InstallDir "$INSTDIR"'
Pop $0
Pop $1
CreateDirectory "$LOCALAPPDATA\LiteRustBrowser\diagnostics"
FileOpen $4 "$LOCALAPPDATA\LiteRustBrowser\diagnostics\install-helper.log" w
FileWrite $4 "$0$\r$\n$1"
FileClose $4
SetErrorLevel $0
SectionEnd
