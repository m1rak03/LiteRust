# 0.15.0-alpha.2

Полноэкранный режим F11/Esc, корректное открытие новой вкладки поверх панелей, устранение источников белой вспышки, компактные аккаунты, категории настроек и дополнительные действия боковой панели. Подробнее: [заметки выпуска](docs/releases/0.15.0-alpha.2.md).

# 0.15.0-alpha.1 — локальная разработка

- 330 тем, 30 форм вкладок, поиск тем и настраиваемые градиенты.
- Меню официальных аккаунтов Google, Яндекс, Microsoft и Yahoo; показ при первом или каждом запуске.
- Панель инструментов: поиск и дублирование вкладок, заметки, калькулятор, печать/PDF.
- Нейтральная кнопка «Аккаунты» вместо недостоверного статуса «Войти».
- Справка по TLS и прямой переход на сайт медицинской аттестации без отключения проверки сертификатов.
- Новое оформление документации и шаблоны GitHub.

Windows-установщик не проверен; публикация этой версии не подтверждена.

## 0.14.0-alpha.2 — локальная версия

- Контраст текста для своего акцентного цвета и единое оформление полей настроек.
- Ползунки не теряют выбранное значение при фоновом обновлении интерфейса.
- Проверка записи и чтения расширенных настроек через файл профиля.
- Актуальная документация и упаковка исходников без GitHub.

## 0.14.0-alpha.1 — расширенные настройки

- Акцент, размер текста, скругление, три компоновки новой вкладки, карточки, ссылки Google, формат часов, размытие и заполнение фона.
- Домашняя страница, режим запуска, ссылки в фоне, масштаб страниц и выключение записи истории, включая обновление заголовков старых записей.
- Поиск настроек, раздельный сброс и сохранение вводимого адреса при обновлениях интерфейса.
- Сохранение сессии при перезапуске VPN и ограничение небезопасных домашних адресов.

## 0.13.0-alpha.2 — защита

- DPAPI для метаданных/резервных копий, CSP nonce для панели, ограничения WebView2, системные пути утилит. См. SECURITY.md.

## 0.11.0-alpha.1

- Google-style simplified new tab: Google search, bookmarks as shortcuts, services menu, top-right sign-in, optional speech input, Lens/Gemini links.
- Top-left Google account entry opens the persistent LiteRust profile; no permanent-session or OAuth-sync claim.
- Dock on any edge with persistent position and full content-area settings/VPN panels.
- Happ-inspired VPN workspace, strict URI/subscription import, Windows DPAPI vault, bundled Xray, explicit restart, proxy fail-closed behavior and user-triggered HTTPS RTT check.
- Portable and full-runtime packages now include the VPN engine and its license/source notice.

## 0.10.0-alpha.2

- Проверка установленного WebView2 Runtime до создания native window; понятная ошибка и адрес официального установщика.
- Журнал этапов запуска в LocalAppData/LiteRustBrowser/diagnostics/startup.log.
- Новый каталог запускателя, прежний профиль браузера сохранён.

## 0.10.0-alpha.1

- Five visual designs and 55 bundled themes; validated, persistent profile preferences.
- Resizable appearance picker, keyboard focus, light/dark/contrast filters, internal-page theming.
- Compact toolbar IPC and stable tab DOM across progress updates; no duplicate start-page load.
- Explicit background WebView unloading, protecting active/loading/downloading pages.
- Official Google Account launcher alongside existing Password Manager portal; no OAuth or password-sync claim.
- Portable Windows x64 cross-build and updated UI/profile tests. See docs/UPDATE_0_10.md for verification limits.

# 0.9.0-alpha.1 — 2026-11-11

Предварительная стабилизация исходного 0.8, не стабильный релиз.

- Исправлены Wry PermissionResponse API и некомпилируемый тест истории.
- Добавлены Cargo.lock, fmt/check/Clippy в CI и платформонезависимая test library.
- Защищены профиль, резервная копия и миграция; OS lock, fail-closed восстановление,
  bounded read, сообщения ошибок сохранения/запуска.
- Автовыгрузка opt-in; активные загрузки защищены от LRU и закрытия вкладки.
- Native download dialog и ID операций вместо URL-корреляции, clear не отменяется callbacks.
- Исправлены capacity/dedup закладок и поиск библиотеки.
- Усилены URL input, IPC Source, CSP; добавлены числовые порты/IDN tests.
- Отключено новое сохранение паролей WebView2, внешний Google portal сохранён.
- 28 core tests и локальный Chromium UI test выполнены; MSVC check/Clippy проходят.
- EXE не слинкован, Windows runtime/сайты/производительность не проверены.
