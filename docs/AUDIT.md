# Аудит 0.8 → 0.9.0-alpha.1

Дата: 11.11.2026. Изучены все 20 файлов исходного ZIP, включая Rust, UI,
PowerShell, CI и документацию. Приоритет: P0 — сборка/данные/граница доверия,
P1 — повседневное использование, P2 — развитие. Наличие кода не означает
успешную проверку Windows. Ссылки на участки даны по именам функций, чтобы
они оставались актуальны после rustfmt.

| Приоритет | Файл / участок | Причина и последствия | Исправление | Проверка |
|---|---|---|---|---|
| P0 | `src/main.rs::site_permission_policy` | В закреплённом Wry нет варианта Prompt: браузер не компилировался | Default для камеры/микрофона/геолокации/захвата, неизвестные разрешения Deny; Allow нигде не возвращается | Исходник Wry, Windows check/Clippy; реальные prompts не проверены |
| P0 | `src/storage.rs::tests::tab_navigation_isolated_and_persistent` | `b.go_back()` требовал mutable binding | Исправлен тест | Реальный Linux cargo test |
| P0 | `storage::load`, `profile::ProfileLock` | При порче обеих копий возвращался default, следующий UI event перезаписывал данные | Возврат ошибки; резервное восстановление; исходные байты сохраняются перед clean | Регрессионные тесты восстановления/двух повреждённых копий/миграции |
| P0 | `profile::ProfileLock`, `main::run` | Два процесса одновременно писали один JSON/tmp и теряли изменения | Блокировка ОС на срок процесса; понятная ошибка второго запуска | Linux lock/release test; Windows компиляция, runtime не проверен |
| P1 | `storage::save`, `profile::atomic_write` | Прямой fs::copy мог оборвать резервную копию | Запись+sync+замена для основного и резервного файла, уборка временного файла при ошибке | Roundtrip/recovery; отключение питания, quota, ACL на Windows не проверены |
| P1 | `storage::read_data` | Проверка metadata перед fs::read оставляла рост файла между проверками | Чтение через take(limit+1) и повторная проверка | Код и Clippy; профиль ограничен 8 MiB |
| P1 | `storage::clean` | u64::MAX и saturating_add создавали совпадающие ID | Перенумерация ID при старте, ID сессии уникальны | Тест hostile IDs, legacy JSON |
| P0 | `main::show_active/enforce_page_budget`, UI | LRU без согласия терял формы/медиа; смена бюджета немедленно удаляла views | Auto-discard opt-in, default false, подтверждения в UI; загрузки исключены из LRU | Playwright accept/cancel, проверка Windows-кода; сохранность реальных форм требует Windows |
| P1 | `windows_platform::configure`, `storage::record_download_state` | Wry SetHandled(true) скрывал интерфейс; URL не идентифицирует параллельные загрузки | Native dialog, ID операции, обработка состояний/пути; callbacks после clear игнорируются | COM сигнатуры и Windows check; тест одинаковых URL и clear |
| P1 | `main::CloseTab/ToggleCosmeticFilter` | Уничтожение WebView могло прервать загрузку | Активную загрузку сначала нужно закончить/отменить; LRU её пропускает | Статическая проверка/компиляция; runtime ещё нужен |
| P1 | `storage::toggle_bookmark` | 1001-я закладка молча удаляла первую | Отказ с уведомлением; нормализация URL и дедупликация | Capacity/canonical/import tests |
| P1 | `ui/library.html`, `main::render_library` | `.entry{display:flex}` перебивал hidden; поиск пропускал записи вне первых 300 | Явное `[hidden]`, отображение всех ограниченных коллекций | Chromium поиск с кириллицей |
| P0 | `security::reject_address_input`, `main::navigate` | Userinfo URL превращался в поисковый запрос и мог раскрыть пароль поисковику | Отказ до поиска; опасные схемы, длина, controls | Регрессионные тесты |
| P1 | `navigation::destination` | example.org:8443 ошибочно трактовался как неизвестная схема | Распознавание числового порта, IPv6, IDN | Реальный тест |
| P0 | `main::make_view/load_active`, IPC | about:blank из сайта оставлял старый адрес в панели; источник IPC не проверялся | about:blank только для локальной загрузки; фильтрация первоначальных blank callbacks; IPC только about:blank у отдельной панели | Windows check; hostile navigation runtime ещё нужен |
| P1 | `ui/*` | Не было дополнительной CSP-границы для локальных документов | default-src none, запрет base/form; текстовые вставки и escaping сохранены | UI Chromium + existing escaping tests |
| P1 | `windows_platform::configure` | Нельзя выдавать локальное сохранение паролей WebView2 за Google | Выключены новые password autosave и general autofill; старые сохранённые пароли не удаляются | Документация Settings4 и компиляция; старое password autofill этим API не отключается |
| P1 | `main::persist/main` | Ошибки release-сборки видны только в отсутствующей консоли | Startup MessageBox, notice об ошибке сохранения; URL убран из navigation error log | Компиляция; визуально в Windows не проверено |
| P1 | Cargo/CI | Нет lockfile в поставке; CI не проверял fmt/check/Clippy | Cargo.lock, полный Windows pipeline, Linux core job | MSVC cross-check/Clippy и core tests выполнены |

## Сохранённые полезные свойства

Только WebView панели имеет IPC handler; сайт не получает внутренние команды.
HTML библиотеки экранирует строки, UI использует textContent, payload для
`evaluate_script` сериализуется serde_json. Page/Title/Popup события несут tab_id
и generation, закрытые и пересозданные WebView отбрасываются. Экспорт использует
create_new и не заменяет прежний экспорт. Private не читает обычный JSON.

## Неустранённые ограничения и риски

- `main::on_page_event/on_title`: generation защищает жизненный цикл WebView,
  но не является NavigationId. Быстрые переходы/редиректы внутри одной живой
  WebView, SPA и последовательность title/load требуют Windows stress tests.
  Полная нативная история, NavigationCompleted/ошибки TLS ещё не реализованы.
- Popup открывается отдельной вкладкой с Deny исходному запросу. JS window.open
  не получает полноценный window/opener: OAuth и некоторые приложения ломаются.
- Горячие клавиши реализованы в DOM панели: при фокусе внутри сайта они не
  гарантируются. Нужен Windows AcceleratorKeyPressed без открытия IPC сайтам.
- Файловая запись и HTML-импорт синхронны на UI-потоке; лимиты ограничивают объём,
  но медленный диск может вызвать паузу. Следующий этап — сериализованный writer
  с flush при выходе и обратной связью об ошибках, без гонки с profile lock.
- Сбой питания/полный диск, файловые ACL и антивирусные блокировки Windows не
  воспроизводились. Копирование для миграции требует дополнительного места.
  Блокировка действует между новыми экземплярами, не со старым v0.8.
- Приватность cookies/кэша, DPI, multi-monitor, GPU, audio, fullscreen, clipboard,
  screen capture, WebRTC, DRM и системный proxy требуют Windows runtime tests.
- Разрешения основаны на native default prompt и Deny. Собственной панели
  разрешений по origin/отзыва, редактора cookies и нескольких профилей нет.
- Загрузки: filename, Save As, конфликты имён, отмена и большие файлы переданы
  стандартному WebView2 UI. Не подтверждены реальным скачиванием. Нет собственного
  progress/cancel API, пауза/возобновление не обещаны; закрытие всего приложения
  может прервать загрузки. Ошибки объединены в «Прервано / ошибка».
- JSON содержит приватные метаданные открытым текстом, сохраняются backup и
  pre-v09 архив. Очистка истории не является безопасным стиранием всех резервных
  копий и WebView2-профиля. Метаданные ограничены, но это не encrypted vault.
- Нет сертифицированного security audit, Safe Browsing интеграции, подписанных
  обновлений и доказательства отсутствия иных критических уязвимостей.

## Рекламный фильтр

`network_block.rs` использует 16 CDP glob-правил, изменения асинхронны.
Network.enable и Network.setBlockedURLs не ожидают подтверждения до навигации.
First request, redirects, frames и Service Workers не покрыты доказанно;
быстрые переключения могут применяться с задержкой. Ошибки completion логируются.
Доменные исключения точные (тесты не допускают subdomain/notexample совпадений).
Cosmetic filter отдельный, исключения сетевого фильтра к нему не относятся.
Подписки EasyList не добавлялись. План полноценного фильтра: bounded immutable
ruleset; parser с точным origin/resource-type/exception semantics; WebResourceRequested
с разбором URL, а не подстроками; callback completion и generation/revision;
выключение на origin; cache с лимитом; тестовый набор false positives; замеры CPU.
Перед включением EasyList требуется проверить лицензию конкретного списка и
парсера, атрибуцию, обновления и совместимость правил. Нельзя называть этот
эксперимент полной блокировкой рекламы или защитой от фишинга.

## Архитектура движка

WebView2 сохраняет Chromium-совместимость и обновления runtime, но ограничивает
контроль браузерной истории, Google integration, extensions и host UI. CEF даёт
больше контроля, требует поставки/обновления Chromium и не решает GPM автоматически.
Servo — исследовательская альтернатива с существенно иным покрытием сайтов;
переход сейчас увеличит риск совместимости. Движок не заменён. Android/Linux
требуют отдельных платформенных интеграций, не просто замены target.
