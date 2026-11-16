param([Parameter(Mandatory=$true)][string]$InstallDir)
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath((Join-Path $InstallDir 'literust-browser.exe'))
foreach ($process in @(Get-Process -Name 'literust-browser' -ErrorAction SilentlyContinue)) {
    try { $path = $process.Path } catch { continue }
    if ($path -and [IO.Path]::GetFullPath($path) -eq $target) {
        if (-not $process.CloseMainWindow()) { exit 1 }
        if (-not $process.WaitForExit(15000)) { exit 1 }
    }
}
exit 0
