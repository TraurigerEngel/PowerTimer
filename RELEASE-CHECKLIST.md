# PowerTimer 0.8.0 Stable — чек-лист выпуска

- [ ] `npm.cmd run check` — все тесты успешно.
- [ ] `cargo test --manifest-path .\src-tauri\Cargo.toml` — все Rust-тесты успешно.
- [ ] `npm.cmd run build` — успешная оптимизированная сборка.
- [ ] В `Release\PowerTimer_0.8.0_Stable` находится только `PowerTimer.exe`.
- [ ] EXE запускается отдельно от исходников (на Windows 11 с WebView2 Runtime).
- [ ] Тестовый режим: интервал 10 секунд, предупреждение 5 секунд, плавный прогресс без скачка.
- [ ] Отмена, сворачивание в трей, восстановление по клику, выход из приложения — работают.
- [ ] После «Выход» `Get-Process PowerTimer -ErrorAction SilentlyContinue` ничего не выводит.
- [ ] `Artifacts`: Portable ZIP, Source ZIP, SHA256SUMS.txt, RELEASE-METADATA.json.
- [ ] Публиковать как Stable только после перечисленных проверок.

**Внимание:** тестировать таймер с включённым «Тестовый режим», чтобы не выключать компьютер.
