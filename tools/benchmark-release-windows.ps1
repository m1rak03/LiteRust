param([Parameter(Mandatory=$true)][string]$OldExe,[Parameter(Mandatory=$true)][string]$NewExe,[string]$OutputPath='packaging/modernization-benchmark.csv')
$ErrorActionPreference='Stop'
$rows=[System.Collections.Generic.List[object]]::new()
$keys=New-Object -ComObject WScript.Shell
foreach($version in @(@('0.15.0-alpha.4',$OldExe),@('0.16.0-alpha.1',$NewExe))){
 for($repeat=1;$repeat -le 3;$repeat++){
  $log="$env:LOCALAPPDATA/LiteRustBrowser/diagnostics/startup.log"
  Remove-Item $log -ErrorAction SilentlyContinue
  $clock=[Diagnostics.Stopwatch]::StartNew()
  $app=Start-Process $version[1] -ArgumentList '--private' -PassThru
  try {
   $ready=$false
   for($i=0;$i -lt 150;$i++){Start-Sleep -Milliseconds 200;$app.Refresh();if($app.HasExited){throw 'Benchmark browser exited'};if($app.MainWindowHandle -ne 0 -and (Test-Path $log) -and (Get-Content $log -Raw) -match 'Main toolbar IPC ready'){$ready=$true;break}}
   if(-not $ready){throw 'Benchmark startup timed out'}
   $startup=$clock.Elapsed.TotalMilliseconds
   if(-not $keys.AppActivate($app.Id)){throw 'Cannot focus benchmark window'}
   Start-Sleep -Seconds 1
   $tabsClock=[Diagnostics.Stopwatch]::StartNew()
   1..4 | ForEach-Object {$keys.SendKeys('^t');Start-Sleep -Milliseconds 750}
   $openElapsed=$tabsClock.Elapsed.TotalMilliseconds
   Start-Sleep -Seconds 3
   for($sample=1;$sample -le 3;$sample++){
    $app.Refresh()
    if($app.HasExited){throw 'Benchmark browser exited before sampling'}
    $rootStarted=$app.StartTime
    # Windows retains parent PIDs after parents exit and may reuse them for our browser.
    # Older unrelated processes must not be mistaken for its descendants.
    $all=@(Get-CimInstance Win32_Process | Where-Object { $_.CreationDate -ge $rootStarted.AddMilliseconds(-1) })
    $ids=[Collections.Generic.HashSet[int]]::new();[void]$ids.Add($app.Id)
    $born=@{};$born[[int]$app.Id]=$rootStarted
    do{$changed=$false;foreach($item in $all){if($ids.Contains([int]$item.ParentProcessId) -and $item.CreationDate -ge $born[[int]$item.ParentProcessId].AddMilliseconds(-1) -and $ids.Add([int]$item.ProcessId)){$born[[int]$item.ProcessId]=$item.CreationDate;$changed=$true}}}while($changed)
    [long]$privateBytes=0;[long]$workingBytes=0;$count=0
    foreach($childId in $ids){$process=Get-Process -Id $childId -ErrorAction SilentlyContinue;if($process -and [math]::Abs(($process.StartTime-$born[[int]$childId]).TotalMilliseconds) -lt 10){$privateBytes+=$process.PrivateMemorySize64;$workingBytes+=$process.WorkingSet64;$count++}}
    $rows.Add([pscustomobject]@{Version=$version[0];Scenario='5 private internal tabs; default live budget';Repeat=$repeat;Sample=$sample;StartupReadyMs=[math]::Round($startup);FourTabCommandsMs=[math]::Round($openElapsed);PrivateMiB=[math]::Round($privateBytes/1MB,2);SumWorkingSetMiB=[math]::Round($workingBytes/1MB,2);Processes=$count})
    Start-Sleep -Seconds 2
   }
  } finally {if(-not $app.HasExited){[void]$app.CloseMainWindow();if(-not $app.WaitForExit(15000)){Stop-Process -Id $app.Id -Force}}}
  Start-Sleep -Seconds 2
 }
}
$rows | Export-Csv -NoTypeInformation -Encoding utf8 $OutputPath
$rows | Format-Table
Write-Output 'PrivateMiB is committed private memory. Summed working sets double-count shared pages. Startup includes a 200ms polling interval. Tab-command elapsed time includes deliberate waits. This is a CI internal-page scenario, not a general browsing benchmark.'
