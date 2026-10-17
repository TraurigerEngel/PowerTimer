# PowerTimer 0.8.0 — исходная инструкция сборки

Исходники подготовлены на базе подтверждённой **RC5**. Работа таймера и утверждённый интерфейс не менялись; изменены только версия и релизная инфраструктура.

## Как собрать финальный выпуск в Windows 11

Откройте PowerShell из корня распакованного проекта и выполните:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File ".\MAKE-FINAL-RELEASE.ps1"
```

Скрипт устанавливает npm-зависимости, запускает проверки JavaScript и Rust, компилирует EXE и формирует в проекте:

- `Release\PowerTimer_0.8.0_Stable\PowerTimer.exe` — один EXE.
- `Artifacts\PowerTimer_0.8.0_Stable_Portable.zip`.
- `Artifacts\PowerTimer_0.8.0_Stable_Source.zip`.
- `Artifacts\SHA256SUMS.txt` и `RELEASE-METADATA.json`.

Архив исходников собирается из локальных файлов проекта. В GitHub-редакции бинарные web-шрифты отсутствуют; используется установленный Inter либо системный шрифт. В ZIP не включаются `node_modules`, `target`, журналы и настройки.

**Важно:** приложения на Tauri в Windows зависят от системного WebView2 Runtime. Перед публичным релизом обязательно выполнить ручной чек-лист `RELEASE-CHECKLIST.md`. Не запускать реальные системные действия при тестировании.

## Ручная сборка без упаковки

```powershell
npm.cmd install
npm.cmd run check
cargo test --manifest-path .\src-tauri\Cargo.toml
npm.cmd run build
```

Готовый файл: `src-tauri\target\release\powertimer.exe`.
