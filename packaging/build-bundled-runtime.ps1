param(
  [Parameter(Mandatory=$true)][string]$BrowserExe,
  [Parameter(Mandatory=$true)][string]$XrayExe,
  [Parameter(Mandatory=$true)][string]$WebViewLoaderDll,
  [Parameter(Mandatory=$true)][string]$RuntimeInstaller
)
$ErrorActionPreference = 'Stop'
$env:LITERUST_XRAY_PAYLOAD = (Resolve-Path $XrayExe).Path
$env:LITERUST_BROWSER_PAYLOAD = (Resolve-Path $BrowserExe).Path
$env:LITERUST_LOADER_PAYLOAD = (Resolve-Path $WebViewLoaderDll).Path
$env:LITERUST_RUNTIME_INSTALLER = (Resolve-Path $RuntimeInstaller).Path
rustc --edition 2021 -D warnings -C opt-level=s -C strip=symbols -C panic=abort "$PSScriptRoot\bundled-runtime.rs" -o "$PSScriptRoot\LiteRust-0.11.0-alpha.2-Full-Setup-x64.exe"
if ($LASTEXITCODE -ne 0) { throw 'Bundled runtime build failed' }
