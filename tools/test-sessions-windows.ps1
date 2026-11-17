param([Parameter(Mandatory=$true)][string]$Exe)
$ErrorActionPreference='Stop'
Remove-Item (Join-Path $env:LOCALAPPDATA 'LiteRustSessionTest') -Recurse -Force -ErrorAction SilentlyContinue
$fixture=Join-Path $env:RUNNER_TEMP 'literust-session-fixture'
New-Item -ItemType Directory -Force $fixture | Out-Null
'<!doctype html><title>Session fixture</title><p>Synthetic storage test</p>' | Set-Content (Join-Path $fixture 'index.html')
$server=Start-Process python -ArgumentList '-m','http.server','37643','--bind','127.0.0.1','--directory',$fixture -PassThru -RedirectStandardOutput (Join-Path $fixture 'http.log') -RedirectStandardError (Join-Path $fixture 'http-error.log')
try {
  Start-Sleep -Seconds 2
  foreach($phase in @('write-a','read-a','empty-b','write-b','read-b','read-a')) {
    $test=Start-Process $Exe -ArgumentList '--session-self-test',$phase -PassThru
    if(-not $test.WaitForExit(45000)){Stop-Process -Id $test.Id -Force;throw "Storage phase timed out: $phase"}
    $test.Refresh()
    if($test.ExitCode -ne 0){throw "Storage phase failed: $phase"}
    Start-Sleep -Seconds 2
  }
  Get-Content "$env:LOCALAPPDATA/LiteRustSessionTest/results.log"
} finally { Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue }
