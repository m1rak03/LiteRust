param(
    [Parameter(Mandatory=$true)][string]$Baseline,
    [Parameter(Mandatory=$true)][string]$Candidate,
    [ValidateRange(5,25)][int]$Repeats = 5
)
$ErrorActionPreference = 'Stop'
$temp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$temp = (Resolve-Path -LiteralPath $temp).Path
$root = (New-Item -ItemType Directory (Join-Path $temp ('literust-startup-load-' + [Guid]::NewGuid().ToString('N')))).FullName
$builds = @(
    @{ Role = 'baseline'; Exe = (Resolve-Path -LiteralPath $Baseline).Path },
    @{ Role = 'candidate'; Exe = (Resolve-Path -LiteralPath $Candidate).Path }
)

function Measure-Startup([hashtable]$Build, [int]$Run) {
    $fixture = Join-Path $root $Build.Role
    $logFile = Join-Path $fixture 'LiteRustBrowser/diagnostics/startup.log'
    if (Test-Path -LiteralPath $logFile) { Remove-Item -LiteralPath $logFile }
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $Build.Exe
    $info.Arguments = '--profile main'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.EnvironmentVariables['LOCALAPPDATA'] = $fixture
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $app = [Diagnostics.Process]::Start($info)
    try {
        $ui = $null
        $page = $null
        $log = ''
        while ($clock.Elapsed.TotalSeconds -lt 15) {
            if ($app.HasExited) { throw 'Browser exited during startup' }
            if (Test-Path -LiteralPath $logFile) {
                $file = [IO.File]::Open($logFile, 'Open', 'Read', 'ReadWrite')
                $reader = [IO.StreamReader]::new($file)
                try { $log = $reader.ReadToEnd() } finally { $reader.Dispose() }
                if ($null -eq $page -and $log.Contains('New tab document ready')) {
                    $page = $clock.Elapsed.TotalMilliseconds
                }
                if ($null -eq $ui -and $log.Contains('Toolbar JavaScript ready')) {
                    $app.Refresh()
                    if ($app.MainWindowHandle -ne [IntPtr]::Zero) {
                        $ui = $clock.Elapsed.TotalMilliseconds
                    }
                }
                if ($null -ne $ui -and $null -ne $page -and
                    (!$log.Contains('startup rust_entered=') -or $log.Contains('startup first_document_loaded='))) { break }
            }
            Start-Sleep -Milliseconds 16
        }
        if ($null -eq $ui -or $null -eq $page) { throw 'Startup timed out before the toolbar and first page were ready' }
        if ($log.Contains('startup rust_entered=') -and !$log.Contains('startup first_document_loaded=')) {
            throw 'Startup trace missed the first completed document'
        }
        $phases = @{}
        foreach ($match in [regex]::Matches($log, '(?m)^startup ([a-z_]+)=([0-9]+)us')) {
            $phases[$match.Groups[1].Value] = [Math]::Round(([long]$match.Groups[2].Value / 1000), 2)
        }
        [pscustomobject]@{
            Role = $Build.Role
            Version = [regex]::Match($log,'^LiteRust ([^ ]+) startup').Groups[1].Value
            Runtime = [regex]::Match($log,'(?m)^WebView2 Runtime available: ([^\r\n]+)').Groups[1].Value
            Run = $Run
            UiMs = [Math]::Round($ui, 1)
            FirstPageMs = [Math]::Round($page, 1)
            RustEntryMs = $phases['rust_entered']
            ProfilesOpenedMs = $phases['profiles_opened']
            RuntimeCheckedMs = $phases['runtime_checked']
            SettingsLoadedMs = $phases['settings_loaded']
            WindowCreatedMs = $phases['window_created']
            ToolbarCreatedMs = $phases['toolbar_created']
            InterfaceCreatedMs = $phases['interface_created']
            InterfaceVisibleMs = $phases['interface_visible']
            PageControllerReadyMs = $phases['page_controller_ready']
            FirstDocumentLoadedMs = $phases['first_document_loaded']
        }
    } catch {
        if (Test-Path -LiteralPath $logFile) { Get-Content -LiteralPath $logFile | Write-Host }
        throw
    } finally {
        if (!$app.HasExited) {
            [void]$app.CloseMainWindow()
            if (!$app.WaitForExit(12000)) {
                $app.Kill()
                [void]$app.WaitForExit(10000)
            }
        }
        $app.Dispose()
    }
}

try {
    foreach ($build in $builds) {
        $fixture = Join-Path $root ($build.Role + '/LiteRustBrowser')
        New-Item -ItemType Directory -Path $fixture -Force | Out-Null
        $settings = Join-Path $fixture 'browser-data.json'
        [IO.File]::WriteAllText($settings, '{"account_prompt_seen":true}', [Text.UTF8Encoding]::new($false))
        Measure-Startup $build 0 | Out-Null
    }
    for ($i = 1; $i -le $Repeats; $i++) {
        foreach ($build in $builds) { Measure-Startup $build $i }
    }
} finally {
    $resolved = (Resolve-Path -LiteralPath $root).Path
    if ([IO.Path]::GetDirectoryName($resolved) -eq $temp -and
        [IO.Path]::GetFileName($resolved).StartsWith('literust-startup-load-')) {
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
