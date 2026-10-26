# NotOffice 1.0.0-rc.2 для Linux

Портативная сборка для **Debian/Ubuntu x86_64** (и других дистрибутивов с
X11/Wayland и OpenGL). Бинарник статический (`x86_64-unknown-linux-musl`),
поэтому не требует установленной Rust-среды.

## Запуск

```bash
tar -xzf NotOffice-1.0.0-rc.2-linux-x86_64.tar.gz
cd NotOffice-1.0.0-rc.2-linux-x86_64
chmod +x notoffice
./notoffice
```

Можно передать файл: `./notoffice ~/Документы/пример.docx`.

Настройки, recent и recovery хранятся в `$XDG_DATA_HOME/notoffice`
(по умолчанию `~/.local/share/notoffice`). Переменная `LITEOFFICE_DATA_DIR`
переопределяет каталог.

Ярлык: скопируй `notoffice.desktop` в `~/.local/share/applications/`,
иконку — в `~/.local/share/icons/hicolor/256x256/apps/notoffice.png`.

## Что работает

- DOCX/DOC (DOC — чтение, сохранение в DOCX), XLS/XLSX, TXT/MD/RTF;
- RU/EN, undo/redo, поиск/замена, recovery;
- открытие/сохранение через системные диалоги.

## Чего нет в Linux-сборке

- **Печать** — только Windows (в Linux вернётся понятная ошибка).
- **Просмотр/печать PDF** — только Windows 10/11; PDF-файл откроется с
  сообщением, что растровый просмотр недоступен.
- Нативная интеграция с проводником/ассоциации файлов не настраивается
  автоматически.

## Проверка

Сборка скомпилирована и проверена как ELF x86-64; **запуск на реальном
Debian/Ubuntu в этой сессии не выполнялся**. Если что-то не запускается,
проверь наличие `libx11-6`, `libxkbcommon0`, `libwayland-client0`, `libgl1`,
`libfontconfig1`.
