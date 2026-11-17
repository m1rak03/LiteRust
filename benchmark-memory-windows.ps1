# LiteRust vs Chrome process-tree memory sampling (no synthetic benchmark scores).
# Start both browsers manually, open the same sites, wait for all pages to load.
# Example:
#   .\benchmark-memory-windows.ps1 -LiteRustProcessId 1234 -ChromeProcessId 5678 -Samples 12 -IntervalSeconds 5
# Each row is a point-in-time process TREE sample. The sum of working sets
# double-counts shared pages. PrivateMiB is private committed memory, not resident RAM.
param(
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$LiteRustProcessId,
  [Parameter(Mandatory=$true)][ValidateRange(1,2147483647)][int]$ChromeProcessId,
  [Parameter(Mandatory=$true)][string]$Scenario,
  [ValidateRange(1,100)][int]$Repeat = 1,
  [ValidateRange(2,200)][int]$Samples = 12,
  [ValidateRange(1,3600)][int]$IntervalSeconds = 5,
  [string]$OutputPath = (Join-Path $PSScriptRoot 'memory-samples.csv')
)
$ErrorActionPreference = 'Stop'
if ($LiteRustProcessId -eq $ChromeProcessId) { throw 'Use distinct root process IDs.' }
if (-not (Get-Process -Id $LiteRustProcessId -ErrorAction SilentlyContinue)) { throw 'LiteRust PID not found.' }
if (-not (Get-Process -Id $ChromeProcessId -ErrorAction SilentlyContinue)) { throw 'Chrome PID not found.' }

$rootStarts = @{
  LiteRust = (Get-Process -Id $LiteRustProcessId).StartTime.Ticks
  Chrome = (Get-Process -Id $ChromeProcessId).StartTime.Ticks
}
$previousCpu = @{}
$previousTime = @{}
$clock = [System.Diagnostics.Stopwatch]::StartNew()

function Get-ProcessTreeIds {
  param([int]$RootId, [object[]]$AllProcesses)
  $ids = [System.Collections.Generic.HashSet[int]]::new()
  [void]$ids.Add($RootId)
  $changed = $true
  while ($changed) {
    $changed = $false
    foreach ($item in $AllProcesses) {
      $parent = [int]$item.ParentProcessId
      $child = [int]$item.ProcessId
      if ($ids.Contains($parent) -and $ids.Add($child)) { $changed = $true }
    }
  }
  return ,(@($ids))
}

function Get-MemorySnapshot {
  param([string]$Browser, [int]$RootId, [int]$Sample, [object[]]$AllProcesses)
  $root = Get-Process -Id $RootId -ErrorAction Stop
  if ($root.StartTime.Ticks -ne $rootStarts[$Browser]) { throw 'Root PID was reused; discard this run.' }
  $timestamp = $clock.Elapsed.TotalSeconds
  $cpu = @{}
  [double]$deltaCpu = 0
  $ids = Get-ProcessTreeIds -RootId $RootId -AllProcesses $AllProcesses
  [long]$privateBytes = 0
  [long]$workingBytes = 0
  $found = 0
  foreach ($childId in $ids) {
    try {
      $process = Get-Process -Id $childId -ErrorAction Stop
      $identity = "$childId/$($process.StartTime.Ticks)"
      $seconds = $process.TotalProcessorTime.TotalSeconds
      $cpu[$identity] = $seconds
      if ($previousCpu.ContainsKey($Browser) -and $previousCpu[$Browser].ContainsKey($identity)) {
        $deltaCpu += [math]::Max(0, $seconds - $previousCpu[$Browser][$identity])
      }
      $privateBytes += [long]$process.PrivateMemorySize64
      $workingBytes += [long]$process.WorkingSet64
      $found++
    } catch { } # Exiting child between process tree lookup and measurement.
  }
  $cpuPercent = $null
  if ($previousTime.ContainsKey($Browser)) {
    $elapsed = $timestamp - $previousTime[$Browser]
    $cpuPercent = [math]::Round(100 * $deltaCpu / $elapsed / [Environment]::ProcessorCount, 3)
  }
  $previousCpu[$Browser] = $cpu
  $previousTime[$Browser] = $timestamp
  [pscustomobject]@{
    Scenario = $Scenario
    Repeat = $Repeat
    CpuPercent = $cpuPercent
    Timestamp = (Get-Date).ToString('o')
    Sample = $Sample
    Browser = $Browser
    RootPid = $RootId
    Processes = $found
    PrivateMiB = [math]::Round($privateBytes / 1MB, 2)
    SumWorkingSetMiB = [math]::Round($workingBytes / 1MB, 2)
  }
}

$rows = @()
for ($sample = 1; $sample -le $Samples; $sample++) {
  $allProcesses = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
  $rows += Get-MemorySnapshot -Browser 'LiteRust' -RootId $LiteRustProcessId -Sample $sample -AllProcesses $allProcesses
  $rows += Get-MemorySnapshot -Browser 'Chrome' -RootId $ChromeProcessId -Sample $sample -AllProcesses $allProcesses
  if ($sample -lt $Samples) { Start-Sleep -Seconds $IntervalSeconds }
}
$rows | Export-Csv -Path $OutputPath -NoTypeInformation -Encoding UTF8
Write-Host "Memory samples saved: $OutputPath"
foreach ($browser in @('LiteRust','Chrome')) {
  $sorted = @($rows | Where-Object Browser -eq $browser | Sort-Object PrivateMiB)
  $n = $sorted.Count
  $mid = [int][math]::Floor($n / 2)
  if (($n % 2) -eq 0) {
    $median = ($sorted[$mid - 1].PrivateMiB + $sorted[$mid].PrivateMiB) / 2
  } else { $median = $sorted[$mid].PrivateMiB }
  Write-Host ('{0} median private memory: {1:N2} MiB ({2} samples)' -f $browser,$median,$n)
}
Write-Host 'CPU is machine-normalized and excludes processes absent from either snapshot (underestimates churn). PrivateMiB is committed private bytes, not physical RAM. Repeat each scenario at least three times.'
