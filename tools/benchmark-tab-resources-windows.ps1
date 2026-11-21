param(
    [Parameter(Mandatory=$true)][string]$Baseline,
    [Parameter(Mandatory=$true)][string]$Candidate,
    [ValidateRange(1,10)][int]$Repeats = 2,
    [int[]]$Tabs = @(4,12)
)
$ErrorActionPreference = 'Stop'
$temp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$temp = (Resolve-Path -LiteralPath $temp).Path
$versions = @(
    @{ Executable = (Resolve-Path -LiteralPath $Baseline).Path; Role = 'baseline' },
    @{ Executable = (Resolve-Path -LiteralPath $Candidate).Path; Role = 'candidate' }
)

foreach ($tabCount in $Tabs) {
    for ($run = 1; $run -le $Repeats; $run++) {
        foreach ($version in $versions) {
            $fixture = (New-Item -ItemType Directory (Join-Path $temp ('literust-tab-load-' + [Guid]::NewGuid().ToString('N')))).FullName
            $info = [Diagnostics.ProcessStartInfo]::new()
            $info.FileName = $version.Executable
            $info.Arguments = '--session-self-test tab-load'
            $info.UseShellExecute = $false
            $info.CreateNoWindow = $true
            $info.RedirectStandardError = $true
            $info.EnvironmentVariables['LOCALAPPDATA'] = $fixture
            $info.EnvironmentVariables['LITERUST_SESSION_TEST_ROOT'] = $fixture
            $info.EnvironmentVariables['LITERUST_BENCHMARK_TABS'] = [string]$tabCount
            $app = [Diagnostics.Process]::Start($info)
            $errors = $app.StandardError.ReadToEndAsync()
            try {
                if (!$app.WaitForExit(50000) -or $app.ExitCode -ne 0) {
                    $reason = if ($app.HasExited) { $errors.GetAwaiter().GetResult() } else { 'Process timeout' }
                    throw ("Could not measure " + $version.Role + " at " + $tabCount + " tabs. " + $reason)
                }
                $file = Join-Path $fixture 'tab-load.json'
                $result = Get-Content -LiteralPath $file -Raw | ConvertFrom-Json
                if ($result.tabs -ne $tabCount -or $result.live_tabs -ne $tabCount -or !$result.sample.memory_coverage) {
                    throw 'Invalid benchmark sample'
                }
                [pscustomobject]@{
                    Role = $version.Role
                    Version = $result.version
                    Runtime = $result.runtime
                    Tabs = $tabCount
                    Run = $run
                    PrivateMiB = [Math]::Round($result.sample.private_bytes / 1MB, 1)
                    Processes = $result.sample.process_count
                    MemorySampled = $result.sample.memory_coverage
                    CpuPercent = [Math]::Round($result.sample.cpu_percent, 3)
                    CpuSampled = $result.sample.cpu_coverage
                }
            } finally {
                if (!$app.HasExited) {
                    $app.Kill()
                    [void]$app.WaitForExit(10000)
                }
                $app.Dispose()
                $resolved = (Resolve-Path -LiteralPath $fixture).Path
                if ([IO.Path]::GetDirectoryName($resolved) -eq $temp -and
                    [IO.Path]::GetFileName($resolved).StartsWith('literust-tab-load-')) {
                    Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction SilentlyContinue
                }
            }
        }
    }
}
