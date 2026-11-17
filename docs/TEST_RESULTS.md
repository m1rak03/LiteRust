# Выполненные проверки

11.11.2026, Linux x86_64, Rust/Cargo 1.99.1, Node 24.22.1,
Playwright Chromium 151.0.7922.34. Windows-target установлен через rustup.
Все логи в `audit-logs/`, исходные результаты v0.8 сохранены отдельно.

| Команда | Фактический результат |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --locked --all-targets --target x86_64-pc-windows-msvc` | PASS, Windows typecheck из Linux |
| `cargo clippy --locked --all-targets --target x86_64-pc-windows-msvc -- -D warnings` | PASS, включая binary test target |
| `cargo test --locked --no-default-features --lib` | PASS: 28 passed, 0 failed; core реально исполнялся в Linux |
| `cargo clippy --locked --no-default-features --lib --tests -- -D warnings` | PASS |
| `node tools/check-source.mjs` | PASS, синтаксис/структурные инварианты, не компиляция Rust |
| `node tools/test-ui.cjs` | PASS, локальный UI в Chromium, без WebView2/COM |
| `python3 tools/test-benchmark.py` | PASS, синтетический тест агрегации медиан, не замер браузеров |
| `cargo build --locked --release --target x86_64-pc-windows-msvc` | FAIL/BLOCKED: linker `link.exe` not found, exit 101; EXE не создан |
| `cargo check --locked --all-targets` (Linux desktop default) | FAIL/BLOCKED: glib-2.0 pkg-config dependency отсутствует; Linux GUI не заявлен |
| `cargo test` / `cargo build --release` на Windows | NOT RUN: нет Windows executor/доступного репозитория LiteRust для CI |
| Native Windows запуск, сайты, RAM/CPU, PowerShell sampler | NOT RUN |

Дополнительные проверки исходной версии: `node tools/check-source.mjs` проходил,
но `cargo check --all-targets --target x86_64-pc-windows-msvc` нашёл отсутствующий
PermissionResponse::Prompt и E0596 в тесте (`baseline-windows-check.log`).
Исходный fmt check также не проходил (`baseline-fmt.log`).

28 core tests охватывают импорт, capacity/duplicates, legacy profile, экспорт и
escaping, URL policy/IDN/порты/credentials, download correlation/clear, восстановление,
процессную блокировку/освобождение, ограничение миграции oversized файла, tab IDs.
Тесты из `main.rs` проверены компилятором Windows-target, но **не исполнялись**.

Playwright проверял безопасный вывод HTML-заголовка, адресную команду с кириллицей,
accept/cancel авто-discard, меню на ширине 690 px, регистронезависимый поиск
библиотеки и отсутствие page errors. IPC в тесте заменён mock-функцией.
Скриншоты в `audit-logs/screenshots/` — HTML UI, не работающий браузер на Windows.

Официальные Google/Microsoft страницы из GOOGLE_PASSWORD_MANAGER.md получены
по HTTPS с HTTP 200. Публичная документация и исходники закреплённых crate
использованы для сверки API, не заменяют integration tests.

## Воспроизведение Windows

Установить Rust stable MSVC, Visual Studio C++ Build Tools + Windows SDK,
WebView2 Runtime, Node. Запустить `build-windows.ps1`; при успехе EXE будет
`target/release/literust-browser.exe`. Затем пройти WINDOWS_SMOKE_TEST.md.
GitHub workflow содержит все проверки и uploads EXE только после успешной
сборки. Workflow подготовлен, запуск и artifact не подтверждены.

## Пять следующих задач

1. Пройти настоящий Windows CI и ручной запуск/навигацию/вкладки/профиль/private/downloads.
2. Native NavigationId/SourceChanged/NavigationCompleted и полноценная история,
   POST/SPA, ошибки подключения/TLS, popup opener и глобальные hotkeys.
3. Проверить Windows privacy/permissions, hostile IPC/navigation, WebView2 storage
   и аварийные сценарии диска; затем сделать UI разрешений/cookies по origin.
4. Испытать native downloads (same URL, cancel/network failure/large files) и
   вынести сохранение профиля в последовательный writer с безопасным shutdown.
5. Провести минимум три повтора RAM/CPU/startup/idle/video/churn против Chrome;
   оптимизировать только подтверждённые профилировщиком расходы.

GPM остаётся отдельным обязательным BLOCKED требованием и не подменён.
