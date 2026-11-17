# Google Password Manager — BLOCKED

Обязательное требование продукта сохранено. Другой менеджер не выбран.
Кнопка 🔑 открывает фиксированный `https://passwords.google.com/` системным
обработчиком Windows во внешнем браузере. Это управление хранилищем Google,
**не автозаполнение, синхронизация, импорт Chrome-паролей или passkeys в LiteRust**.
В приватном окне кнопка не запускает обычный внешний браузер.

## Проверенные официальные источники (11.11.2026)

Страницы получены непосредственно по HTTPS с HTTP 200. Отсутствие открытого
API нельзя доказать одной страницей; в проверенной публичной документации
поддерживаемого способа подключения хранилища GPM к произвольному WebView2
браузеру не найдено. Поэтому продуктовый статус остаётся BLOCKED.

1. [Google: Manage passwords](https://support.google.com/chrome/answer/95606?hl=en)
   описывает Google Password Manager в Chrome, сохранение, просмотр, аккаунт
   и passkeys. Не описывает API чтения/синхронизации vault для LiteRust.
2. [Microsoft: WebView2 browser features](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/browser-features)
   явно отмечает отключённую функцию browser sync. Cookies/profile WebView2
   не становятся профилем Chrome после входа в Google.
3. [Microsoft: ICoreWebView2Settings4](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2settings4)
   документирует IsPasswordAutosaveEnabled и IsGeneralAutofillEnabled.
   В LiteRust оба выключены для новых WebView. **IsPasswordAutosaveEnabled=false
   запрещает новое сохранение, но не отменяет автоподстановку уже существовавших
   паролей WebView2.** Такие данные не удаляются при обновлении. Это отдельное
   хранилище и не интеграция Google; его использование не предлагается как замена.
4. [Google OAuth policies](https://developers.google.com/identity/protocols/oauth2/policies)
   запрещают направлять Google OAuth authorization requests в embedded user-agent
   под контролем разработчика. Нельзя обещать вход Gmail/Docs/OAuth, обходить
   блокировки User-Agent или переносить cookies из внешнего браузера.

## Допустимые варианты

- Сохранить внешний официальный сайт для просмотра/управления паролями.
  Работает в поддерживаемом Google интерфейсе; LiteRust не получает доступ к vault.
- Для сценариев с официальным Google автозаполнением использовать поддерживаемый
  Google браузер/устройство. Это ограничение текущего прототипа, не замена GPM.
- Следить за официальным API/системным credential provider, который прямо
  поддерживает GPM и сторонние Windows-браузеры. Наличие Windows WebAuthn или
  сайта с passkeys само по себе не даёт LiteRust синхронизацию Google passkeys.
- Любой другой временный менеджер — отдельное решение пользователя; не внедрён.

Токены, cookies и Chrome Login Data не читаются; приватные API не используются;
пароли не идут через наш сервер и не хранятся в JSON. Несколько Google-аккаунтов
во внешнем портале не означают multi-account sync в LiteRust.
