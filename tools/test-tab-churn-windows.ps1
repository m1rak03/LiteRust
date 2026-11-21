param(
    [Parameter(Mandatory=$true)][string]$Exe,
    [ValidateRange(2,24)][int]$Tabs = 24,
    [ValidateRange(2,400)][int]$Cycles = 300
)
$ErrorActionPreference = 'Stop'
$temp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$temp = (Resolve-Path -LiteralPath $temp).Path
$fixture = (New-Item -ItemType Directory (Join-Path $temp ('literust-tab-churn-' + [Guid]::NewGuid().ToString('N')))).FullName
$info = [Diagnostics.ProcessStartInfo]::new()
$info.FileName = (Resolve-Path -LiteralPath $Exe).Path
$info.Arguments = '--session-self-test tab-load'
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardError = $true
$info.EnvironmentVariables['LOCALAPPDATA'] = $fixture
$info.EnvironmentVariables['LITERUST_SESSION_TEST_ROOT'] = $fixture
$info.EnvironmentVariables['LITERUST_BENCHMARK_TABS'] = [string]$Tabs
$info.EnvironmentVariables['LITERUST_BENCHMARK_CYCLES'] = [string]$Cycles
$app = [Diagnostics.Process]::Start($info)
$errors = $app.StandardError.ReadToEndAsync()
try {
    if (!$app.WaitForExit(1000 * (55 + 2 * $Cycles)) -or $app.ExitCode -ne 0) {
        $reason = if ($app.HasExited) { $errors.GetAwaiter().GetResult() } else { 'Fixture timeout' }
        throw "Browser churn fixture failed. $reason"
    }
    $result = Get-Content -LiteralPath (Join-Path $fixture 'tab-load.json') -Raw | ConvertFrom-Json
    if ($result.live_tabs -ne $Tabs -or $result.cycles -ne $Cycles -or $result.checkpoints.Count -ne 2) {
        throw 'Fixture did not complete all churn and measurement checkpoints'
    }
    foreach ($point in @($result.checkpoints) + @($result)) {
        if ($point.live_tabs -ne $Tabs -or $point.sample.memory_coverage -eq 0) {
            throw 'Fixture lost live tab state or process coverage'
        }
        [pscustomobject]@{
            Version = $result.version
            Runtime = $result.runtime
            Tabs = $Tabs
            ClosedAndReopened = $point.cycles
            ElapsedSeconds = [Math]::Round($point.elapsed_ms / 1000, 2)
            PrivateMiB = [Math]::Round($point.sample.private_bytes / 1MB, 1)
            Processes = $point.sample.process_count
            MemorySampled = $point.sample.memory_coverage
        }
    }
} finally {
    if (!$app.HasExited) {
        $app.Kill()
        [void]$app.WaitForExit(10000)
    }
    $app.Dispose()
    $resolved = (Resolve-Path -LiteralPath $fixture).Path
    if ([IO.Path]::GetDirectoryName($resolved) -eq $temp -and
        [IO.Path]::GetFileName($resolved).StartsWith('literust-tab-churn-')) {
        Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
    }
}
