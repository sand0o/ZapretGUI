# ZapretGUI

VIBE CODING!!! (скачивайте и запускайте всё на свой страх и риск, код писал devin.ai на базе claude opus 4.7)

Лёгкий GUI для [`flowseal/zapret-discord-youtube`](https://github.com/Flowseal/zapret-discord-youtube). Одна большая кнопка — старт, ещё раз — стоп. Дизайн вдохновлён [`romanvht/ByeByeDPI`](https://github.com/romanvht/ByeByeDPI).

<img width="422" height="652" alt="изображение" src="https://github.com/user-attachments/assets/40bddb28-97b2-44c5-91b0-2b6eb4f8638d" />


## Возможности

-  **Одна кнопка**. Большая круглая кнопка по центру окна — клик и обход включён, ещё клик — выключен.
-  **Лёгкий**. Tauri (Rust + системный WebView2) — итоговый `.exe` около 5–10 МБ, ОЗУ в простое < 50 МБ.
-  **Любая стратегия**. Автоматически находит все `general*.bat` в папке zapret и предлагает выбрать.
-  **Игровой фильтр**. Переключает обход для нестандартных TCP/UDP-портов (для игр), как `service.bat → Game Filter`.
-  **Системный трей**. Окно сворачивается в трей, обход продолжает работать в фоне.
-  **Автостарт обхода** при открытии приложения (опционально).
-  **Тёмная / светлая тема**.

## Как пользоваться

1. Сначала скачайте zip архив запрета с [официального релиза zapret-discord-youtube](https://github.com/Flowseal/zapret-discord-youtube/releases/latest) и распакуйте куда-нибудь без кириллицы и пробелов в пути (например, `C:\zapret-discord-youtube`).
2. Теперь скачайте мой `ZapretGUI-Setup.exe` со [страницы релизов](https://github.com/sand0o/ZapretGUI/releases).
3. Запустите ZapretGUI **от имени администратора** (драйвер WinDivert требует прав админа). При первом запуске:
   - Откройте «Настройки» → «Папка zapret-discord-youtube» → добавьте папку которую вы скачали и распаковали из шага 1.
   - Выберите стратегию (по умолчанию `general`).
4. Закройте настройки и нажмите большую кнопку по центру.

## Сборка

### Требования

- [Rust](https://rustup.rs/) ≥ 1.77 (`rustup target add x86_64-pc-windows-msvc` на Windows).
- На Windows — [WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (предустановлен на Win10 21H2+ / Win11).
- На Linux — `libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`.

### Команды

```sh
# Установить tauri-cli один раз
cargo install tauri-cli --version "^2.0" --locked

# Запуск в режиме разработки (Windows / Linux / macOS)
cd src-tauri
cargo tauri dev

# Релизная сборка (Windows)
cd src-tauri
cargo tauri build
# → src-tauri/target/release/zapret-gui.exe              (portable)
# → src-tauri/target/release/bundle/nsis/*.exe           (installer)
```

CI (`.github/workflows/build.yml`) собирает Windows-бинарник на каждый push в `main` и при пуше тэга `v*` прикрепляет артефакты к GitHub Release.

## Архитектура

```
src-tauri/                    # Tauri (Rust) backend
  src/
    main.rs                   # bootstrap
    lib.rs                    # tauri commands, tray, main loop
    settings.rs               # persistence (%APPDATA%\ZapretGUI\config.json)
    strategies.rs             # парсер general*.bat → аргументы winws.exe
    zapret.rs                 # spawn / stop winws.exe, статус через sysinfo
  capabilities/default.json   # Tauri 2 permissions
  tauri.conf.json
  Cargo.toml
ui/                           # frontend (vanilla HTML / CSS / JS)
  index.html
  style.css
  app.js
.github/workflows/build.yml   # CI: cargo check + Windows bundle
```

### Что делает кнопка «Старт»

1. Читает выбранный `general*.bat` из папки zapret.
2. Парсит строку `start "..." winws.exe ...` (со склеиванием `^`-продолжений), вытаскивает аргументы `winws.exe`.
3. Подставляет переменные:
   - `%BIN%` → `<zapret>\bin\`
   - `%LISTS%` → `<zapret>\lists\`
   - `%GameFilterTCP%`, `%GameFilterUDP%`, `%GameFilter%` — из выбранного режима игрового фильтра.
4. Запускает `winws.exe` напрямую, без видимого консольного окна (`CREATE_NO_WINDOW`), сохраняет PID.
5. Кнопка «Стоп» — `taskkill /F /PID <pid>` плюс контрольный `taskkill /F /IM winws.exe`.

Никакие `service.bat`, обновления и установка как Windows-сервис не используются — GUI намеренно lightweight.

## Лицензия

MIT — см. [`LICENSE`](LICENSE).

ZapretGUI не содержит самих бинарников `winws.exe` / `WinDivert*` — он только запускает то, что вы скачали с [`flowseal/zapret-discord-youtube`](https://github.com/Flowseal/zapret-discord-youtube).
