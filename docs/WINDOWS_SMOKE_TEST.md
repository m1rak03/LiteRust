# Windows приёмка 0.9-alpha.1 (ещё не выполнена)

Выполнять на копии профиля и тестовых аккаунтах. Зафиксировать Windows build,
DPI/мониторы, GPU, Chrome stable и WebView2 Runtime versions, SHA256 EXE.
`build-windows.ps1` должен пройти полностью; затем проверить фактический запуск.
Компиляция CI не заменяет следующие проверки.

## Обязательные сценарии

1. Первый запуск: toolbar/start page, русские символы, 690×400 и 1280×800,
   resize, 100/150/200% DPI, перенос между мониторами, mouse/keyboard focus.
2. Адрес/поиск: example.org, example.org:8443, localhost, IDN, запрос по-русски;
   отказ `javascript:`, `file:`, `data:`, userinfo URL, управляющих символов,
   >8192 bytes; никакой отправки userinfo URL поисковику.
3. Back/Forward/reload/home и HTTP/HTTPS, redirect chain, TLS expired/self-signed,
   offline/DNS failure. Не обходить TLS ошибки. Записать поведение native error page.
4. Создать/выбрать/закрыть активную и неактивную вкладку, закрыть последнюю,
   достичь 24 и попытаться открыть 25-ю. 100 повторов, быстрые переходы, поздние
   Title/Page/Popup от закрытой/recreated view не меняют другие вкладки.
5. По умолчанию auto_discard=false: заполнить формы в 6+ вкладках, переключать,
   убедиться в сохранении. Затем принять предупреждение и проверить 1/3/5 бюджет,
   reload выгруженных, предупреждение при снижении бюджета. Аудио/видео и формы
   после discard теряются по объявленному контракту, не должны считаться сохранёнными.
6. Скачивание: малый/большой файл, два одинаковых URL параллельно с разными путями,
   одинаковые имена, native Save As/cancel, ошибка сети, закрытие приложения.
   Сверить путь/статус/байты. Активная загрузка не выгружается LRU; CloseTab
   предлагает закончить/отменить. Clear history не удаляет файл и completion не
   возвращает запись. Пауза/возобновление LiteRust не заявлены.
7. Закладки: add/remove, эквивалентные URL, limit 1000 без потери первой;
   Chrome HTML entities/unsafe schemes/duplicates/limit 1.5 MB, экспорт и повторный
   импорт Chrome. Папки не сохраняются. Поиск по всем 1000 закладкам/1500 visits.
8. Профиль: legacy JSON, restart, второе окно обычного процесса (отказ), отдельное
   private window. На копии — primary corrupt, both corrupt (отказ без перезаписи),
   oversized file, read-only/full-disk, forced termination во время save. Сверить
   original/pre-v09/backup, отсутствие потерянных исходных данных.
9. Private: тестовая авторизация/cookies/localStorage/IndexedDB/service workers
   в обычном и приватном окне; повторный private запуск, отсутствие обычной истории,
   независимость сессий. Проверить каталог WebView2, network/cache артефакты;
   скачанные файлы остаются на диске. Не называть режим анонимностью.
10. Permissions: camera/mic/geolocation должны пройти native decision flow,
    deny сохраняется ожидаемо; notifications/clipboard read/MIDI/sensors/automatic
    downloads/FileSystemAccess denied. Screen capture chooser/отмена, subframes,
    remembered grants и неизвестные разрешения. Никакого автоматического Allow.
11. IPC hostile page: `window.ipc`, попытки postMessage команд, target blank,
    about:blank, data/file/javascript, redirects и HTML titles с `<script>`.
    Страница не должна управлять toolbar/файлами. Проверить toolbar CSP и Source.
12. CDP: DevTools/network capture до/после toggle и исключения, разные домены,
    redirect, iframe/SW, выключение. Не считать CSS disappearance доказательством
    сетевой блокировки. Зафиксировать early-request race/false positives.
13. 🔑 открывает только внешний passwords.google.com; private не открывает.
    Автозаполнения Google нет. Не использовать токены Chrome или обход OAuth/DRM.
14. Proxy Windows и authenticated proxy, HTTPS, WebSocket reconnect, WebGL,
    WebRTC test peers, file upload, fullscreen/video/audio, clipboard/user gesture.

## Матрица сайтов — все UNVERIFIED

Для каждого записать PASS/FAIL/BLOCKED и версию Runtime: Google Search, YouTube,
Gmail, Google Docs, Google Drive, GitHub, Wikipedia, Reddit, Discord Web,
Telegram Web, Twitch, тестовый интернет-магазин, тестовые login/upload/WebSocket/
WebRTC/complex JS страницы. Для Google OAuth встроенный user-agent может быть
запрещён официальной политикой; нельзя маскировать обходом. DRM зависит от
runtime/контента/лицензии, воспроизведение не обещано.

Колонки протокола: rendering, login/session restart, audio/video, WebGL, forms,
file upload/download, fullscreen, clipboard, popup/opener, TLS errors, OAuth,
DRM, комментарий/лог. Не входить в реальные критичные аккаунты ради теста.

RAM/CPU, cold/warm startup, 1/5/10/20 tabs, video, idle, leak/resource release:
см. PERFORMANCE.md; минимум три независимых повтора основных сценариев.

## Критерий выпуска

Все критерии пользователя для 0.9 должны подтверждаться Windows запуском,
приёмкой и тестами; обнаруженные критические проблемы устранены. Пока этого нет,
имя версии остаётся **0.9.0-alpha.1**. GPM остаётся отдельным BLOCKED требованием.
