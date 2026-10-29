# NotOffice 1.0.1 — релизный комплект

`C:\NotOffice\NotOffice-1.0.1` — **самостоятельный комплект**: исходники,
готовая сборка, документация и доказательства проверок.

Windows 1.0.0 остаётся принятым и сохранён в `accepted-windows-1.0-backup`.
1.0.1 — отдельная сборка с вставкой диапазона и действиями «Недавние»; Windows 10/11 x64.
Версии 1.1–1.2 удалены при очистке; принятый 1.0.0 сохранён в accepted-windows-1.0-backup.
Linux/macOS остаются экспериментальными. Актуальная инструкция: USER_GUIDE_RU.md.
Статусы приёмки и границы доказательств: STATUS_RU.md.

## Описание

> Для тех кто заебался от вечных подписок на самые базовые функции работы с самыми популярными форматами документов, полностью бесплатно, навайбкожено, быстрый и легкий клиент, наслаждайтесь
>
> For those who are sick and tired of endless subscriptions for the most basic features with the most popular document formats — completely free, vibe-coded, a fast and lightweight client. Enjoy.

## Быстрый запуск

```
NotOffice-1.0.1.exe
```

Запускать можно без аргументов (стартовый экран) или передать путь к документу.
EXE портативный: настройки, недавние файлы и recovery создаются рядом с EXE,
поэтому запускай из папки, доступной для записи. Второй вариант —
`NotOffice-1.0.1-portable.zip` — только необходимое: EXE, его `.sha256`, юзергайд и лицензии.

## Что внутри

| Путь | Содержимое |
|---|---|
| `NotOffice-1.0.1.exe` | готовая release-сборка |
| `NotOffice-1.0.1-portable.zip` | минимальный портативный архив: EXE + `.sha256` + USER_GUIDE + лицензии |
| `src/`, `tests/`, `Cargo.toml`, `Cargo.lock` | полные исходники Windows 1.0 |
| `README_REDESIGN_RU.md` | описание интерфейса, RU/EN, измерений |
| `CHANGELOG_1.0.1_RU.md` | изменения 1.0.1 и ограничения |
| `RANGE_PASTE_RU.md` | вставка диапазона, лимиты и read-only защита |
| `THEMES_RU.md` | темы, сохранение, границы влияния |
| `CLEAN_COPY_RU.md` | что переносится и что теряется в пассивной копии |
| `RC_MANUAL_REGRESSION_RU.md` | протокол ручной регрессии печати/PDF/двух окон |
| `RECOVERY_CHECK_RU.md` | протокол проверки восстановления TXT |
| `USER_GUIDE_RU.md` | краткий юзергайд: запуск, форматы, горячие клавиши, печать, recovery |
| `PROJECT_LAYOUT_RU.md` | согласованный формат проекта и правила упаковки |
| `PORTABILITY_RU.md` | Linux/macOS: что собрано, что работает, ограничения |
| `packages/linux/` | Linux tar.gz и `.deb` (Debian/Ubuntu x86_64) |
| `packaging/` | скрипты сборки Linux/macOS и CI workflows |
| `GPT_HANDOFF_RU.md` | **основной документ для будущих правок**: архитектура, инварианты, команды, подводные камни |
| `docs/` | исходная документация проекта: roadmap, Phase 4, требования, история |
| `screenshots/` | скриншоты интерфейса |
| `evidence/` | PDF-выходы печати, скриншоты диалогов, fixture DOC, логи тестов |
| `STATUS_RU.md` | **актуальный статус проекта** |
| `LICENSE` | MIT для кода NotOffice |
| `THIRD_PARTY_LICENSES.md` | аудит лицензий зависимостей |
| `licenses/` | тексты MIT/Apache/BSD/BSL/OFL/UFL и шрифтов |
| `SHA256SUMS.txt` | контрольные суммы всех файлов комплекта |

## Проверки

- `cargo test --all-targets`: **239 passed, 9 ignored**.
- Linux: `cargo check --target x86_64-unknown-linux-gnu` и release-сборка
  `x86_64-unknown-linux-musl` — PASS; запуск на реальном Debian не проверялся.
- macOS: `cargo check --target x86_64-apple-darwin` — PASS; `.app` не собран
  (нужен Mac/CI).
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`: PASS.
- Ручная регрессия принятого кандидата: печать DOCX (предпросмотр, системный диалог), GUI-вывод
  PDF (файл `evidence/rc-manual/printer-user-print.pdf`), просмотр/печать PDF,
  два окна и конфликт сохранения, recovery, legacy DOC с изображением — PASS.
- Физическая печать, portable на чистом Windows без Office и сети, ручное
  восстановление MD/RTF/XLSX и два окна XLSX подтверждены пользователем — PASS.
  Доказательства GUI кандидата сохранены с исходными обозначениями версии.
- Сборка 1.0.1 отличается от 1.0.0 вставкой диапазона, темами и
  документацией; логика чтения/записи форматов не менялась. UX-правки:
  «Правки», «Масштаб», RU/EN справа, экспорт в TXT, точки recovery,
  пустой текстовый документ с «Выделить всё» и поиском.
- Linux-пакеты в packages/linux — прежние экспериментальные RC; в финальный
  Windows portable они не входят и как стабильный Linux 1.0 не заявляются.

## Контрольные суммы

Хэши EXE и ZIP-архивов — в соседних файлах `*.sha256`; полный список по всем
файлам комплекта — `SHA256SUMS.txt`.

## Сборка из исходников

```powershell
$buildTemp = Join-Path $PWD '.build-tmp'
New-Item -ItemType Directory -Path $buildTemp -Force | Out-Null
$env:TMP = $env:TEMP = $buildTemp
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

`target/` в комплект не входит: после первой сборки он занимает несколько ГБ.
Готовый EXE появляется в `target\release\notoffice.exe`; версия — 1.0.1.
