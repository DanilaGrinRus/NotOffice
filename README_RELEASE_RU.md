# NotOffice 1.0.2 — Windows-комплект

Исправление сохранения настроек между независимыми окнами и актуализация
документации. Новые функции не добавлены.

## Назначение

> Для тех кто заебался от вечных подписок на самые базовые функции работы с самыми популярными форматами документов, полностью бесплатно, навайбкожено, быстрый и легкий клиент, наслаждайтесь

> For those who are sick and tired of endless subscriptions for the most basic features with the most popular document formats — completely free, vibe-coded, a fast and lightweight client. Enjoy.

## Запуск

Распакуйте NotOffice-1.0.2-portable.zip и запустите NotOffice-1.0.2.exe.
Нужна Windows 10/11 x64 и папка с правом записи. Office и сеть не требуются.
Настройки и recovery создаются рядом с EXE. Автоматическая установка ассоциаций
файлов не выполняется.

## В комплекте

- NotOffice-1.0.2.exe — release-сборка; копия также находится в C:\NotOffice.
- NotOffice-1.0.2-portable.zip — EXE, инструкция, лицензии и хэш EXE.
- NotOffice-1.0.2-sources.zip — исходники, тесты, документация и evidence.
- USER_GUIDE_RU.md — инструкция пользователя.
- STATUS_RU.md — актуальные возможности, ограничения и результаты.
- CHANGELOG_1.0.2_RU.md и REPORT_1_0_2_RU.md — это исправление.
- THEMES_RU.md, CLEAN_COPY_RU.md, RANGE_PASTE_RU.md — существующие функции.
- docs/ — история предыдущих версий и протоколов.
- SHA256SUMS.txt и .sha256 — контрольные суммы.

## Проверки

fmt/check/test/clippy/release — PASS; 245 passed, 9 ignored.
Проверена одновременная запись четырьмя процессами. Новый GUI-прогон 1.0.2
не выполнялся; предыдущие подтверждения не переименованы в новые.
Подробности: STATUS_RU.md и evidence/test-logs/cargo-*-final.log.

## Платформы и ограничения

Windows 1.0.0 и 1.0.1 сохранены для отката. Linux/macOS остаются
экспериментальными; приложенные Linux RC-пакеты не являются новой сборкой.
Полная совместимость Office не обещается. Функция очистки не является
антивирусом; формулы не вычисляются. Подробности в инструкции пользователя.

## Сборка

Из папки исходников выполните:
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release

Результат: target/release/notoffice.exe. Применяйте короткую временную папку,
если слишком длинный путь TEMP мешает работе MSVC.
