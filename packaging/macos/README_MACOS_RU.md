# NotOffice 1.0.0 для macOS — экспериментально

Готового `.app`/`.dmg` в комплекте нет: macOS-бинарник нельзя собрать на
Windows без Apple SDK и линкера. Исходники компилируются под
`x86_64-apple-darwin` (проверено `cargo check`), но финальную сборку нужно
делать на Mac или в CI.

## Сборка на macOS

Требования: macOS 12+, Xcode Command Line Tools, Rust stable.

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash packaging/macos/build-macos.sh
```

Скрипт создаёт `dist/NotOffice.app` и, если доступен `hdiutil`,
`dist/NotOffice-1.0.0.dmg`. Для Apple Silicon и Intel можно собрать
два `.app` и объединить через `lipo`.

## Данные

`~/Library/Application Support/NotOffice` (или `LITEOFFICE_DATA_DIR`).

## Ограничения

- печать и просмотр PDF — только Windows;
- подпись/нотаризация не выполняются; для распространения нужен Apple
  Developer ID;
- запуск на Mac в этой сессии не проверялся.
