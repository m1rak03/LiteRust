param([Parameter(Mandatory=$true)][string]$Exe)
$ErrorActionPreference='Stop'
$reportRoot=Join-Path $env:LOCALAPPDATA 'LiteRustSessionTest'
$fixture=Join-Path $env:RUNNER_TEMP ('literust-session-fixture-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $fixture | Out-Null
'<!doctype html><title>Session fixture</title><p>Synthetic storage test</p>' | Set-Content (Join-Path $fixture 'index.html')
$server=Start-Process python -ArgumentList '-m','http.server','37643','--bind','127.0.0.1','--directory',('"' + $fixture + '"') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $fixture 'http.log') -RedirectStandardError (Join-Path $fixture 'http-error.log')
$previousTestRoot=$env:LITERUST_SESSION_TEST_ROOT
$env:LITERUST_SESSION_TEST_ROOT=Join-Path $fixture 'storage'
try {
  Start-Sleep -Seconds 2
  foreach($phase in @('write-a','read-a','empty-b','write-b','read-b','empty-private','write-private','empty-private','read-a','read-b')) {
    $test=Start-Process $Exe -ArgumentList '--session-self-test',$phase -WindowStyle Hidden -PassThru
    if(-not $test.WaitForExit(45000)){Stop-Process -Id $test.Id -Force;throw "Storage phase timed out: $phase"}
    $test.Refresh()
    if($test.ExitCode -ne 0){throw "Storage phase failed: $phase"}
    Start-Sleep -Seconds 2
  }
  New-Item -ItemType Directory -Force $reportRoot | Out-Null
  Copy-Item (Join-Path $env:LITERUST_SESSION_TEST_ROOT 'results.log') (Join-Path $reportRoot 'results.log')
  Get-Content (Join-Path $reportRoot 'results.log')
} finally {
  $env:LITERUST_SESSION_TEST_ROOT=$previousTestRoot
  Stop-Process -Id $server.Id -Force -ErrorAction SilentlyContinue
}
