# Промт для передачи LiteOffice в новую среду

Ты работаешь над LiteOffice — самостоятельным Windows-приложением на Rust.
Распакованный комплект содержит исходники в `liteoffice\` и Git bundle
`liteoffice-phase0.git.bundle`.

Сначала полностью прочитай в папке проекта:

1. `CODEX_MASTER_PLAN.md`;
2. `START_HERE.md`;
3. `RELEASE_PROGRESS.md`;
4. этот файл.

Контекст передачи:

- версия пакета — 0.7.1, Rust edition 2021;
- Phase 0 завершена и помечена Git-тегом `stabilized-baseline`;
- последняя ветка: `release/stabilization`;
- восстановлены `src/legacy_layout.rs` и `PrintBlock::PageBreak`;
- проверены `cargo fmt --check`, `cargo check --all-targets`, `cargo test`,
  `cargo test --all-targets` и `cargo build --release`;
- не считай бинарный DOC Word-совместимым: его раскладка является
  приближённой; известные ограничения перечислены в `RELEASE_PROGRESS.md`.

Выполни аудит реального состояния перед любыми изменениями:

```powershell
cd <каталог-распаковки>\liteoffice
git status --short --branch
cargo --version
rustc --version
cargo fmt --check
cargo check --all-targets
cargo test
```

Не удаляй пользовательские документы, настройки, `.lo-layout`, `target` или
неотслеживаемые файлы; не запускай `git clean` и не выполняй очистку. Не
переходи к следующей фазе без явного запроса пользователя. Если пользователь
попросит продолжить, следуй только следующей разрешённой фазе из master plan,
делай небольшие проверяемые коммиты и обновляй `RELEASE_PROGRESS.md`.

## Восстановление Git-истории из bundle

Если исходная папка извлечена без каталога `.git`, восстанови отдельный
репозиторий рядом с ней:

```powershell
cd <каталог-распаковки>
git clone .\liteoffice-phase0.git.bundle LiteOffice-repository
cd .\LiteOffice-repository
git switch release/stabilization
git tag --list stabilized-baseline
```

В этом репозитории исходники лежат в подкаталоге `liteoffice\`, потому что
исходный Git root находился уровнем выше папки проекта. Пользовательские файлы
из исходного `C:\NotOffice` намеренно не входят в bundle и архив.
