$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
  node tools/check-source.mjs
  if ($LASTEXITCODE -ne 0) { throw 'UI source checks failed' }
  cargo fmt --all -- --check
  if ($LASTEXITCODE -ne 0) { throw 'Formatting failed' }
  cargo check --locked --all-targets
  if ($LASTEXITCODE -ne 0) { throw 'cargo check failed' }
  cargo clippy --locked --all-targets -- -D warnings
  if ($LASTEXITCODE -ne 0) { throw 'Clippy failed' }
  cargo test --locked
  if ($LASTEXITCODE -ne 0) { throw 'Tests failed' }
  cargo build --locked --release
  if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
  Copy-Item vendor/xray/xray.exe target/release/xray.exe -Force
  Copy-Item vendor/xray/LICENSE target/release/XRAY-LICENSE.txt -Force
  Copy-Item vendor/xray/SOURCE.txt target/release/XRAY-SOURCE.txt -Force
  $binary = Join-Path $PSScriptRoot 'target\release\literust-browser.exe'
  if (-not (Test-Path $binary)) { throw 'EXE missing' }
  Write-Host "Built $binary. Run docs/WINDOWS_SMOKE_TEST.md before calling this release stable."
} finally { Pop-Location }
