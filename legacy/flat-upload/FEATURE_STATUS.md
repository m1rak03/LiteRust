# Статусы функций 0.9-alpha.1

`WORKING` здесь означает фактически выполненную проверку с указанной областью,
а не готовность браузера для ежедневного использования.

| Статус | Возможности | Граница подтверждения |
|---|---|---|
| WORKING | JSON roundtrip/recovery, fail-closed повреждённый профиль, lock/release, исходная migration copy | 28 Linux unit tests; не crash/power-loss Windows |
| WORKING | URL validation/поиск/порт/IDN, отбрасывание credentials и опасных схем | Unit tests |
| WORKING | Плоские закладки, HTML import/export escaping, duplicate/capacity, история URL и ограничения коллекций, download ID/clear | Unit tests логики |
| WORKING | Поиск по HTML библиотеке, textContent заголовков, address IPC mock, confirm/cancel автовыгрузки, меню 690 px | Chromium Playwright; не WebView2 |
| WORKING | Форматирование, MSVC typecheck всех targets, Windows Clippy -D warnings | Cross-target Linux; не бинарная сборка |
| IMPLEMENTED / UNVERIFIED | Запуск окна/закрытие, сайты HTTP(S), reload/home, resize/DPI/multi-monitor, focus | Нужен Windows WebView2 |
| IMPLEMENTED / UNVERIFIED | До 24 вкладок, восстановление URL, generation checks, 1/3/5 opt-in LRU, Low memory hint, сохранение форм без discard | Реальные формы, ресурсы и события не проверены |
| IMPLEMENTED / UNVERIFIED | Native download dialog, файл/путь/конфликты/отмена, подсчёт активных загрузок | COM компиляция; реальные загрузки ещё не выполнялись |
| IMPLEMENTED / UNVERIFIED | Приватные WebView и отсутствие записи обычного JSON, очистка site data | Нет Windows forensic/session test |
| IMPLEMENTED / UNVERIFIED | Camera/mic/geolocation default prompt; Deny для ряда API, CSP/IPC source boundary | Windows prompt и hostile-page tests ещё нужны |
| IMPLEMENTED / UNVERIFIED | Экспериментальный CDP filter, точные исключения, cosmetic filter | Payload/exception unit tests не доказывают блокировку сети |
| IMPLEMENTED / UNVERIFIED | Внешняя кнопка Google Password Manager | Фиксированный URL протестирован; ОС запуск не проверен |
| BROKEN / BLOCKED | Windows EXE и полный стабильный релиз 0.9 | Нет MSVC linker/SDK и runtime/CI проекта |
| BROKEN / BLOCKED | Google vault autofill/sync/multi-account/passkeys в LiteRust | Официальная интеграция не найдена |
| BROKEN / BLOCKED | Полная Back/Forward семантика POST/SPA/scroll, popup opener/OAuth | URL-only история и Deny original popup не эквивалентны Chrome |
| BROKEN / BLOCKED | Горячие клавиши при фокусе в странице | Сейчас привязаны к DOM панели |
| BROKEN / BLOCKED | Гарантированная фильтрация первого запроса, SW/iframes | CDP асинхронный best effort |
| NOT IMPLEMENTED | Папки закладок; группы/закрепление/поиск вкладок; sidebar | Плоские закладки и обычные вкладки |
| NOT IMPLEMENTED | UI cookies/разрешений по origin, несколько профилей, собственная детальная ошибка сети | Есть только очистка всех site data / native permission policy |
| NOT IMPLEMENTED | Собственные pause/resume/progress загрузок, полноценный native navigation journal | Не имитируются |
| NOT IMPLEMENTED | Переключатель поисковика/темы, UI zoom, extensions, DevTools menu, signed auto-update | Chromium runtime возможности не считаются готовыми фичами оболочки |
| NOT IMPLEMENTED | Синхронизация закладок/вкладок, Android/Linux GUI, Servo, собственная инфраструктура обновлений | Долгосрочные задачи |

Системный proxy, HTTPS/TLS, WebGL, WebSocket, WebRTC, DRM и device permissions
зависят от WebView2/runtime/системы; наличие движка не доказывает работоспособность
в конкретной оболочке. Статус всех реальных сайтов — UNVERIFIED.
