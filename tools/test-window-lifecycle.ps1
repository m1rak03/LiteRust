param([Parameter(Mandatory = $true)][string]$Exe)
$ErrorActionPreference = 'Stop'
$browserPath = (Resolve-Path -LiteralPath $Exe).Path
$temp = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$fixture = (New-Item -ItemType Directory (Join-Path $temp ('literust-window-test-' + [Guid]::NewGuid().ToString('N')))).FullName
$root = Join-Path $fixture 'LiteRustBrowser'
$logFile = Join-Path $root 'diagnostics/startup.log'
$stateFile = Join-Path $root 'browser-data.json'
$processes = [Collections.Generic.List[Diagnostics.Process]]::new()

function Start-FixtureBrowser([string]$Arguments = '--profile main') {
    $info = [Diagnostics.ProcessStartInfo]::new()
    $info.FileName = $browserPath
    $info.Arguments = $Arguments
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.WindowStyle = 'Hidden'
    $info.EnvironmentVariables['LOCALAPPDATA'] = $fixture
    $process = [Diagnostics.Process]::Start($info)
    $processes.Add($process)
    return $process
}

function Wait-Condition([scriptblock]$Condition, [string]$Description) {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        if (& $Condition) { return }
        Start-Sleep -Milliseconds 200
    } while ([DateTime]::UtcNow -lt $deadline)
    if (Test-Path $logFile) { Get-Content -LiteralPath $logFile | Write-Host }
    throw $Description
}

function Test-ReadyWindows([int]$Count) {
    if (-not (Test-Path $logFile)) { return $false }
    $log = Get-Content -LiteralPath $logFile -Raw
    return ([regex]::Matches($log, 'Toolbar JavaScript ready').Count -ge $Count)
}

function Read-FixtureState {
    try {
        $bytes = [IO.File]::ReadAllBytes($stateFile)
        $magic = [Text.Encoding]::ASCII.GetBytes("LiteRustDPAPI1`0")
        $payload = [byte[]]$bytes[$magic.Length..($bytes.Length - 1)]
        $json = [Security.Cryptography.ProtectedData]::Unprotect($payload, $null, 'CurrentUser')
        [Text.Encoding]::UTF8.GetString($json) | ConvertFrom-Json
    } catch {
        return $null
    }
}

try {
    if ($PSVersionTable.PSEdition -eq 'Desktop') {
        Add-Type -AssemblyName System.Security
    } else {
        Add-Type -AssemblyName System.Security.Cryptography.ProtectedData
    }
    $first = Start-FixtureBrowser
    Wait-Condition { (Test-ReadyWindows 1) -and -not $first.HasExited } 'Primary window did not become ready'

    $relay = Start-FixtureBrowser '--profile main --new-window'
    if (-not $relay.WaitForExit(8000) -or $relay.ExitCode -ne 0) {
        throw 'New window request did not reach the existing profile process'
    }
    Wait-Condition { (Test-ReadyWindows 2) -and (Read-FixtureState).additional_windows.Count -eq 1 } 'The second window was not saved'

    # A deliberate interruption of this fixture covers recovery without a shutdown event.
    $first.Kill()
    if (-not $first.WaitForExit(10000)) { throw 'Interrupted fixture process did not exit' }
    Remove-Item -LiteralPath $logFile
    $restored = Start-FixtureBrowser
    Wait-Condition { (Test-ReadyWindows 2) -and -not $restored.HasExited } 'The two windows were not restored after interruption'

    Wait-Condition {
        $restored.Refresh()
        $restored.MainWindowHandle -ne [IntPtr]::Zero
    } 'Restored windows did not become visible'
    if (-not $restored.CloseMainWindow()) { throw 'Could not close one fixture window' }
    Wait-Condition {
        $saved = Read-FixtureState
        $null -ne $saved -and $saved.additional_windows.Count -eq 0 -and -not $restored.HasExited
    } 'Closing one window affected its sibling'
    $restored.Refresh()
    if (-not $restored.CloseMainWindow() -or -not $restored.WaitForExit(10000)) {
        throw 'Closing the last fixture window did not exit the profile process'
    }
    Write-Output 'PASS: ordinary window opening, crash recovery and independent closure'
} finally {
    foreach ($process in $processes) {
        if (-not $process.HasExited) {
            try {
                $process.Kill()
                [void]$process.WaitForExit(5000)
            } catch {
                if (-not $process.HasExited) { throw }
            }
        }
        $process.Dispose()
    }
}
