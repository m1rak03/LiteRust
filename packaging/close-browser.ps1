param([Parameter(Mandatory=$true)][string]$InstallDir)
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath((Join-Path $InstallDir 'literust-browser.exe'))
$logDir = Join-Path $env:LOCALAPPDATA 'LiteRustBrowser\diagnostics'
New-Item -ItemType Directory -Force $logDir | Out-Null
$log = Join-Path $logDir 'install-close.log'
'Starting graceful upgrade shutdown' | Set-Content $log
foreach ($process in @(Get-Process -Name 'literust-browser' -ErrorAction SilentlyContinue)) {
    try { $path = $process.Path } catch { continue }
    if (-not $path -or [IO.Path]::GetFullPath($path) -ne $target) { continue }
    $process.Refresh()
    if ($process.HasExited) { continue }
    # A process can be starting its WebViews before its main window becomes visible.
    for ($attempt=0; $attempt -lt 40; $attempt++) {
        $process.Refresh()
        if ($process.HasExited -or $process.MainWindowHandle -ne 0) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($process.HasExited) { continue }
    "Closing process $($process.Id); window=$($process.MainWindowHandle)" | Add-Content $log
    if (-not $process.CloseMainWindow()) {
        $process.Refresh()
        if ($process.HasExited) { continue }
        'No closeable main window; leaving process intact' | Add-Content $log
        exit 1
    }
    if (-not $process.WaitForExit(30000)) {
        'Graceful shutdown timed out; leaving process intact' | Add-Content $log
        exit 1
    }
    "Process exited: $($process.ExitCode)" | Add-Content $log
}
'Graceful shutdown complete' | Add-Content $log
exit 0
