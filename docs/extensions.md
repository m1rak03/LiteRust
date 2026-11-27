# Расширения WebView2 / WebView2 extensions

Проверка в Windows устанавливает локальное распакованное расширение Manifest V3, отключает его, включает и удаляет. Для этого LiteRust создаёт отдельный каталог данных WebView2 и минимальный манифест без разрешений. После удаления проверяется отсутствие конкретного ID, поскольку в профиле могут находиться системные компоненты среды выполнения.

The native test uses a new WebView2 environment and a generated, permission-free MV3 manifest. It installs, disables, enables and removes the extension, then verifies that its ID is absent from the profile. It does not exercise content scripts, password autofill or extension popups.

## Проверка в Windows / Windows test

Run the following in a fresh PowerShell session at the repository root. The test profile and result stay in the newly created temporary directory.

~~~powershell
cargo build --locked
$fixture = New-Item -ItemType Directory -Path (Join-Path $env:TEMP ([guid]::NewGuid().ToString()))
$env:LOCALAPPDATA = $fixture.FullName
$env:LITERUST_SESSION_TEST_ROOT = $fixture.FullName
$probe = Start-Process ./target/debug/literust-browser.exe -ArgumentList '--session-self-test','extensions' -PassThru -Wait
if ($probe.ExitCode) { throw "WebView2 extension test failed ($($probe.ExitCode))" }
Get-Content (Join-Path $fixture.FullName 'extension-probe/result.txt')
~~~

## Совместимость / Compatibility

Метод AddBrowserExtension принимает папку распакованного расширения. Установка через магазин, интерфейс управления для пользователя и кнопки действий расширений в LiteRust пока отсутствуют.

WebView2 registers an unpacked extension in its profile. The host supplies the extension management UI. Before enabling extensions in browsing profiles, LiteRust needs to isolate trusted internal pages and show requested permissions before installation. Store installation, action popups and third-party password managers remain unverified. [Microsoft WebView2 profile API](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2profile7)
