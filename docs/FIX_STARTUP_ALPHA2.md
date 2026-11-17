# Исправление запуска alpha.2

Пользователь сообщил Windows 11 и отсутствие Edge/WebView2. Runtime — обязательная зависимость Wry/WebView2. Добавлена проверка `wry::webview_version()` (GetAvailableCoreWebView2BrowserVersionString) до EventLoop/WindowBuilder. Ошибка и пустая версия не допускают создание окна; выводится сообщение с официальной ссылкой установки. Системные настройки, GPU и профиль не меняются. Добавлен журнал этапов запуска без регистрации посещаемых URL.

Проверено: Windows GNU release build, Clippy all-targets -D warnings, rustfmt, статическая регрессия порядка Runtime-check → WindowBuilder. Запускатель пересобран с новым payload, проверен PE32+ x64 и системные DLL-импорты. Ссылка установщика проверена: официальный redirect Microsoft, HTTP 200. На Windows исправление пока не запускалось. Runtime не включён в EXE и устанавливается пользователем отдельно.
