# Архитектура

- `main.rs`: Tao window/event loop, отдельная toolbar WebView, HashMap живых
  страниц, команды UI, построение локальной библиотеки. UI и COM в одном STA.
- `storage.rs`: ограниченные коллекции, serde-совместимость старого профиля,
  URL-only история, import/export metadata. `clean` перенумеровывает session IDs.
- `profile.rs`: процессная OS file lock и запись через same-directory replacement.
- `navigation.rs`, `security.rs`: тестируемая интерпретация адресов и URL policy.
- `import.rs`: Netscape HTML разбирается как данные, ограничение 1.5 MB.
- `windows_platform.rs`: документированные COM Settings4 и DownloadStarting /
  StateChanged; callback не захватывает собственную COM operation, чтобы не
  создать reference cycle. Счётчик активных загрузок удерживает view от LRU.
- `network_block.rs`: экспериментальный CDP payload и асинхронный вызов.
- `google_passwords.rs`: фиксированная внешняя ссылка; нет vault API.
- `lib.rs`: core для Linux unit tests без GTK; binary требует feature desktop.

Граница доверия: IPC только у toolbar, её навигация ограничена about:blank;
проверяется Source, тело ограничено 2.1 MB, команды serde enum. У страниц нет IPC.
Заголовки/URL недоверенные: textContent/escape_html/serde_json. CSP локальных HTML
запрещает загрузки ресурсов/формы/base вне разрешённых inline scripts/styles.

События страниц: tab_id+generation проверяются по текущей карте живых views.
Download ID принадлежит операции и не привязан к URL. Очищенный список не
восстанавливается поздними callbacks. ID старых download records обнуляются
при восстановлении, чтобы не совпадать с ID операций нового процесса.

При private запуске не читается BrowserData обычного профиля и нет его profile lock/WebContext; каталог имён локальных профилей читается для интерфейса. Используется
with_incognito(true). Проверка реальной изоляции storage требует Windows.
При обычном запуске lock берётся до чтения профиля и удерживается до завершения
процесса. Журнал не зашифрован; атомарность замены не заменяет резервное копирование.

Выделены только модули, позволяющие тестировать и локализовать ошибки. Большая
перестройка окна/вкладок не проводилась. Следующий архитектурный шаг — native
NavigationId/history и асинхронный одиночный writer, после Windows приёмки.

Модернизация 0.16: `local_profiles.rs` управляет каталогом и изолированными корнями; `home.rs` валидирует сетку и быстрые сайты; `layout.rs` содержит модель шторки. Весь код остаётся в одном desktop-приложении, без серверной инфраструктуры. Сайтам IPC не добавлен. Подробнее: [отчёт](modernization/REPORT.md).
