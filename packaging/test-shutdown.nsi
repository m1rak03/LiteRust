Unicode true
Name "LiteRust shutdown regression"
OutFile "test-shutdown.exe"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\Programs\LiteRust"
Section
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
SetErrorLevel $0
SectionEnd
