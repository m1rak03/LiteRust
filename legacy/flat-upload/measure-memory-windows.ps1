# Capture RAM used by the whole process tree, including msedgewebview2.exe children.
# Examples:
#   .\measure-memory-windows.ps1 -RootPid 15468
#   Get-CimInstance Win32_Process -Filter "Name='literust-browser.exe'" | Select-Object ProcessId
param([Parameter(Mandatory = $true)][ValidateRange(1, 2147483647)][int]$RootPid)
$ErrorActionPreference = 'Stop'
$all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name)
$ids = [System.Collections.Generic.HashSet[int]]::new()
[void]$ids.Add($RootPid)
$changed = $true
while ($changed) {
  $changed = $false
  foreach ($process in $all) {
    if ($ids.Contains([int]$process.ParentProcessId) -and -not $ids.Contains([int]$process.ProcessId)) {
      [void]$ids.Add([int]$process.ProcessId)
      $changed = $true
    }
  }
}
$private = [long]0
$working = [long]0
$found = @()
foreach ($pidToRead in $ids) {
  try {
    $process = Get-Process -Id $pidToRead -ErrorAction Stop
    $private += [long]$process.PrivateMemorySize64
    $working += [long]$process.WorkingSet64
    $found += [pscustomobject]@{
      Id = $pidToRead
      Name = $process.ProcessName
      PrivateMiB = [math]::Round($process.PrivateMemorySize64 / 1MB, 1)
      WorkingSetMiB = [math]::Round($process.WorkingSet64 / 1MB, 1)
    }
  } catch {
    Write-Warning "Process $pidToRead has already exited or is inaccessible."
  }
}
$found | Sort-Object PrivateMiB -Descending | Format-Table -AutoSize
[pscustomobject]@{
  RootPid = $RootPid
  Processes = $found.Count
  TotalPrivateMiB = [math]::Round($private / 1MB, 1)
  SumWorkingSetMiB = [math]::Round($working / 1MB, 1)
} | Format-List
Write-Host 'Note: This is a single snapshot, not a performance benchmark.'
