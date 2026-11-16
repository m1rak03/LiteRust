param(
  [Parameter(Mandatory=$true)][string]$BrowserExe,
  [Parameter(Mandatory=$true)][string]$XrayExe,
  [Parameter(Mandatory=$true)][string]$WebViewLoaderDll
)
$ErrorActionPreference = 'Stop'
$env:LITERUST_XRAY_PAYLOAD = (Resolve-Path $XrayExe).Path
$env:LITERUST_BROWSER_PAYLOAD = (Resolve-Path $BrowserExe).Path
$env:LITERUST_LOADER_PAYLOAD = (Resolve-Path $WebViewLoaderDll).Path
rustc --edition 2021 -C opt-level=s -C strip=symbols -C panic=abort "$PSScriptRoot\launcher.rs" -o "$PSScriptRoot\LiteRust-0.11.0-alpha.1-Windows-x64.exe"
if ($LASTEXITCODE -ne 0) { throw 'Launcher build failed' }
