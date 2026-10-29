# Платформы NotOffice 1.0

Состояние на 26.10.2026. Windows 1.1.0 — текущая сборка; принятый 1.0.0 сохранён отдельно; Linux и macOS —
экспериментальные, без печати и просмотра PDF.

Windows EXE пересобран после исправления XLSB (фильтрация типов листов и
ISO-даты). Приложенные Linux-пакеты относятся к предыдущим исходникам rc.2:
это исправление в них не пересобиралось. Их хэши сохранены; runtime-приёмка
Linux/macOS по-прежнему не выполнена.

## Матрица

| Платформа | Сборка | Что работает | Чего нет |
|---|---|---|---|
| Windows 10/11 x64 | готовая `NotOffice-1.3.0.exe` | всё, включая печать и PDF | — |
| Linux x86_64 (Debian/Ubuntu) | готовый tar.gz и `.deb` | редактирование DOCX/XLSX/TXT/MD/RTF, recovery, RU/EN | печать, просмотр/печать PDF |
| macOS x86_64/arm64 | только исходники и CI-скрипт | компилируется, редактирование | готового `.app` нет; печать/PDF только Windows |

Linux-артефакты ниже сохранены с исходными RC-именами и хэшами. Их нельзя
переименовывать в финальный Linux 1.0 без отдельной сборки и приёмки.

## Linux

Артефакты:

- `packages/linux/NotOffice-1.0.0-rc.2-linux-x86_64.tar.gz`
- `packages/linux/notoffice_1.0.0~rc2-1_amd64.deb`

Бинарник собран как `x86_64-unknown-linux-musl` (статический ELF, PIE) на
Windows через `rust-lld`. Это **Debian-совместимая портативная сборка**;
`dpkg-deb` при сборке не использовался, пакет собран вручную по формату ar.

```bash
tar -xzf NotOffice-1.0.0-rc.2-linux-x86_64.tar.gz
cd NotOffice-1.0.0-rc.2-linux-x86_64
chmod +x notoffice
./notoffice
```

или:

```bash
sudo apt install ./notoffice_1.0.0~rc2-1_amd64.deb
notoffice
```

Данные: `$XDG_DATA_HOME/notoffice` или `~/.local/share/notoffice`.
`NOTOFFICE_DATA_DIR` переопределяет каталог.

Зависимости `.deb`: `libx11-6`, `libx11-xcb1`, `libxcb1`, `libxkbcommon0`,
`libxkbcommon-x11-0`, `libwayland-client0`, `libgl1`, `libegl1`,
`libfontconfig1`, `libxcursor1`, `libxi6`, `libxrandr2`.

**Не проверено:** запуск на реальном Debian/Ubuntu. Проверена только
компиляция и ELF-заголовок; GUI-тест на Linux не выполнялся.

## macOS

`cargo check --target x86_64-apple-darwin` проходит. Готового `.app`/`.dmg`
нет: на Windows нет Apple SDK/линкера, а кросс-линковка macOS требует SDK,
который нельзя распространять в этом комплекте.

Сборка на Mac:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash packaging/macos/build-macos.sh
```

Скрипт создаёт `dist/NotOffice.app` (и `.dmg` при наличии `hdiutil`).
Исходники и workflow `.github/workflows/build-macos.yml` лежат в комплекте.
Подпись/нотаризация не выполняются.

## Общие ограничения не-Windows

- `print.rs`/`pdf_print.rs` возвращают «Printing requires Windows»;
- `pdf.rs` на не-Windows отдаёт «PDF raster viewing requires Windows 10/11»;
- шрифты: Segoe UI на Linux/macOS нет, используется fallback egui; вёрстка
  может отличаться от Windows;
- системные диалоги — через `rfd`; на Linux нужен xdg-desktop-portal или
  GTK-окружение;
- путь данных на не-Windows — per-user, чтобы `.deb`/`.app` работали без
  записи в `/usr/bin` и `/Applications`.

## Проверки

| Команда | Результат |
|---|---|
| `cargo check --target x86_64-apple-darwin` | PASS |
| `cargo check --target x86_64-unknown-linux-gnu` | PASS |
| `cargo build --release --target x86_64-unknown-linux-musl` | PASS |
| `cargo test --all-targets` (Windows) | 239 passed, 9 ignored |
| ELF x86-64, static PIE | PASS (заголовок) |
| запуск Linux GUI | NOT VERIFIED |
| сборка `.app` на macOS | NOT VERIFIED (нет Mac) |

## Что сделать на реальной ОС

1. Linux: распаковать tar, запустить, открыть DOCX/XLSX/TXT, проверить
   диалоги и сохранение; при необходимости поставить недостающие libs.
2. Linux `.deb`: `sudo apt install ./...deb`, проверить ярлык и данные в
   `~/.local/share/notoffice`.
3. macOS: собрать на Mac, проверить `.app`, при необходимости подписать
   Developer ID и нотаризовать.
4. Печать/PDF на не-Windows потребуют отдельной реализации (CUPS/PDFium);
   в 1.0 RC их нет.
