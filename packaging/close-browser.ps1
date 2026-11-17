param([Parameter(Mandatory=$true)][string]$InstallDir)
$ErrorActionPreference = 'Stop'
$target = [IO.Path]::GetFullPath((Join-Path $InstallDir 'literust-browser.exe'))
$logDir = Join-Path $env:LOCALAPPDATA 'LiteRustBrowser\diagnostics'
New-Item -ItemType Directory -Force $logDir | Out-Null
$log = Join-Path $logDir 'install-close.log'
'Starting graceful upgrade shutdown' | Set-Content $log
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class LiteRustUpgradeWindows {
    public delegate bool EnumCallback(IntPtr hwnd, IntPtr state);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumCallback callback, IntPtr state);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr hwnd, StringBuilder name, int max);
    [DllImport("user32.dll", SetLastError=true)] public static extern bool PostMessageW(IntPtr hwnd, uint message, IntPtr w, IntPtr l);
    public static IntPtr[] Find(uint pid) {
        var windows = new List<IntPtr>();
        EnumWindows((hwnd, state) => {
            uint owner; GetWindowThreadProcessId(hwnd, out owner);
            if (owner == pid) {
                var name = new StringBuilder(256); GetClassNameW(hwnd, name, name.Capacity);
                // Tao's default top-level window class; exclude WebView2 helper windows.
                if (name.ToString() == "Window Class") windows.Add(hwnd);
            }
            return true;
        }, IntPtr.Zero);
        return windows.ToArray();
    }
}
'@
foreach ($process in @(Get-Process -Name 'literust-browser' -ErrorAction SilentlyContinue)) {
    try { $path = $process.Path } catch { continue }
    if (-not $path -or [IO.Path]::GetFullPath($path) -ne $target) { continue }
    $process.Refresh()
    if ($process.HasExited) { continue }
    # Do not use Process.MainWindowHandle: WebView2 can expose helper top-level windows.
    $windows=@()
    for ($attempt=0; $attempt -lt 40; $attempt++) {
        $process.Refresh()
        if ($process.HasExited) { break }
        $windows=@([LiteRustUpgradeWindows]::Find([uint32]$process.Id))
        if ($windows.Count -gt 0) { break }
        Start-Sleep -Milliseconds 250
    }
    if ($process.HasExited) { continue }
    "Closing process $($process.Id); browser windows=$($windows.Count)" | Add-Content $log
    if ($windows.Count -eq 0) {
        'No browser window found; leaving process intact' | Add-Content $log
        exit 1
    }
    foreach ($window in $windows) {
        if (-not [LiteRustUpgradeWindows]::PostMessageW($window, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero)) {
            "Cannot post WM_CLOSE: $([Runtime.InteropServices.Marshal]::GetLastWin32Error())" | Add-Content $log
            exit 1
        }
    }
    if (-not $process.WaitForExit(30000)) {
        'Graceful shutdown timed out; leaving process intact' | Add-Content $log
        exit 1
    }
    "Process exited: $($process.ExitCode)" | Add-Content $log
}
'Graceful shutdown complete' | Add-Content $log
exit 0
