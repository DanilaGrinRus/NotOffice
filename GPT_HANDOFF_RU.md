# GPT handoff — NotOffice 1.3.0

Документ для того, кто будет править этот форк дальше. Здесь архитектура,
инварианты, команды, карта тестов, известные ограничения и грабли. Всё
описанное проверено в этой сессии, если не помечено иначе.

## 1. Что это

- Самостоятельный форк **NotOffice 0.8.0-rc.22** с переработанным интерфейсом,
  русским/английским UI и функциями Phase 4, которых не было в апстриме:
  recovery таблиц, undo/redo таблиц и текста, замена во всех редактируемых
  форматах, типизированный экспорт XLSX.
- Версия: **1.3.0**; принятый Windows 1.0.0 сохранён отдельно.
- Текущий проект: `C:\NotOffice\NotOffice-1.3.0`; 1.0.0 — в `accepted-windows-1.0-backup`. Старый roadmap в docs/
  исторический; он не определяет статус финального форка.
- **Windows 1.0 принят пользователем.** Linux/macOS остаются экспериментальными.
  Принятый RC сохранён в C:\NotOffice\accepted-rc2-backup вне поставки.
  После финализации добавлены UX-правки (меню, масштаб, язык, recovery-точка);
  логика чтения/записи форматов не менялась.

## 2. Карта комплекта

| Путь | Что там |
|---|---|
| `src/`, `tests/`, `Cargo.toml`, `Cargo.lock` | полные исходники Windows 1.0 |
| `NotOffice-1.3.0.exe` | release-сборка 1.1.0 |
| `NotOffice-1.3.0-portable.zip` | портативный архив |
| `README_REDESIGN_RU.md` | UI, RU/EN, измерения |
| `PROJECT_LAYOUT_RU.md` | **согласованный формат проекта** — сохранять при дальнейших правках |
| `PORTABILITY_RU.md` | сборка Linux/macOS, ограничения, `.deb`/tar |
| `CHANGELOG_1.1.0_RU.md` | изменения 1.1.0 и Known Limitations |
| `CLEAN_COPY_RU.md` | пассивная копия: что переносится и что теряется |
| `RC_MANUAL_REGRESSION_RU.md` | ручная регрессия печати/PDF/окон |
| `RECOVERY_CHECK_RU.md` | проверка recovery TXT |
| `docs/` | исходная документация: roadmap, Phase 4, FR, security |
| `evidence/print-pdf/` | PDF, созданные штатным print helper |
| `evidence/pdf-print/` | PDF range/scale evidence |
| `evidence/rc-manual/` | скриншоты, `printer-user-print.pdf`, fixture DOC |
| `evidence/test-logs/` | логи cargo test/print/typed export |
| `SHA256SUMS.txt` | хэши всех файлов комплекта |

## 3. Архитектура

Один процесс = одно окно = один активный документ. Состояние — `NotOfficeApp`
в `src/main.rs`; поля `docx`, `text`, `spreadsheet`, `legacy_doc`, `pdf`
взаимоисключающие.

| Модуль | Ответственность |
|---|---|
| `main.rs` | состояние приложения, окно, меню/шорткаты, recovery-оркестрация, undo-диспетчер, replace, печать |
| `app_tests.rs` | unit-тесты NotOfficeApp: open/save, recovery, history, search, workbook, UI |
| `clean_copy.rs` | пассивная DOCX/XLSX-копия: модель → новый пакет, лимиты изображений, проверка |
| `docx_view/` | layout: FlowBlock/пагинация/preview→print; render: абзацы, таблицы, изображения |
| `ui_shell.rs` | командная строка, «Файл/Правки», строка документа, Find/Replace, статус, стартовый экран, язык |
| `ui_theme.rs` | палитра, типографика, иконки, кнопки, рамки |
| `app_paths.rs` | каталог данных: Windows — рядом с EXE; Linux/macOS — XDG/Application Support |
| `docx.rs` | DOCX package/model, сохранение, fingerprints |
| `doc.rs` | legacy Word 97–2003 DOC, текст, best-effort raster gallery |
| `xlsx.rs`, `xls.rs` | импорт таблиц; XLSX распознаёт числа/bool/даты; ODS/XLSB через calamine |
| `csv_import.rs`, `html_export.rs` | CSV/TSV в таблицу; HTML-экспорт DOCX/таблиц |
| `workbook_edit.rs` | values-copy, typed XLSX export, защита оригинала |
| `spreadsheet_ui.rs` | сетка, ввод, форматирование |
| `text_document.rs` | TXT/MD/RTF модель и сохранение |
| `recovery.rs` | сессии, снапшоты, lease, discovery |
| `search.rs`, `replace.rs` | поиск/замена |
| `print.rs`, `pdf_print.rs`, `pdf_view.rs` | печать, PDF-печать, просмотр |
| `i18n.rs` | RU/EN, `tr!`/`tfm!`, файл языка |
| `a4.rs`, `legacy_layout.rs`, `doc_export.rs` | геометрия A4, legacy-разметка, DOCX-экспорт |
| `file_ops.rs`, `preferences.rs`, `recent.rs`, `startup.rs` | открытие, настройки, recent, новое окно |

## 4. Ключевые инварианты

1. **Безопасное сохранение.** DOC, импортированный RTF, копии таблиц и recovery
   требуют Save As; защищённые источники проверяются по путям/алиасам.
   Обычные открытые DOCX/TXT/MD могут сохраняться по текущему пути;
   fingerprint блокирует конфликт внешнего изменения. Не обещать пользователю
   абсолютный запрет перезаписи любых оригиналов.
2. **Recovery-снапшоты неизменяемы.** Пишется `pending-*.tmp`, проверяется
   round-trip, затем rename в `snapshot-<20 digits>.<ext>`; хранится последняя
   валидная точка и предыдущая. Битая точка не удаляет предыдущую.
3. **Recovery root — рядом с EXE** (`NotOffice-recovery`). Сессия:
   `session-<pid>-<nonce>/` с `lease`, `origin.txt`, снапшотами. `lease`
   блокируется ОС; после kill снимается, поэтому сессия становится кандидатом.
4. **Активные окна исключаются** из кандидатов; `discover` пропускает
   залоченные сессии. Пустой lease-only каталог может остаться — это норма.
5. **Восстановление требует Save As.** `recovered_document = true`, `is_dirty`
   учитывает это; после успешного Save As recovery очищается.
6. **Undo/Redo — три независимых стека**: DOCX (`undo_stack`/`redo_stack`),
   таблицы (`workbook_undo`/`workbook_redo`), текст (`text_undo`/`text_redo`).
   Лимит истории — 100. Ctrl+Z/Y диспетчеризуется по активному типу.
7. **Одна транзакция на Replace all.** Лимит 10 000 совпадений блокирует
   замену целиком; частичной записи быть не должно.
8. **XLSX-экспорт типизирован**: числа → `t="n"`, `TRUE/FALSE` → `t="b"`,
   ISO `YYYY-MM-DD` → `t="d"`, остальное → `inlineStr`. Ведущие нули,
   пробелы по краям и формулы остаются текстом. При импорте числа, TRUE/FALSE
и даты (включая числовые даты по стилю) распознаются.
9. **Язык глобальный на процесс**: `AtomicU8`, по умолчанию RU, файл
   `NotOffice-language.txt` рядом с EXE. Новые окна читают выбор; уже
   открытые окна не переключаются живьём. Статусы, созданные до переключения,
   остаются на старом языке до следующего события.
10. **UI**: 44 px командная строка, ~35 px строка документа, 24 px статус;
    A4-страница фиксированной ширины по центру; тёмная тема отключена.
    Верхняя панель: «Файл», «Правки», быстрые действия, «Масштаб», RU/EN.
    Меню «Вид» удалено; параметры печати — отдельная иконка.

## 5. Recovery: где что

- `recovery.rs`:
  - `SessionKind::{Docx, Text, Workbook}`;
  - `snapshot_kind`/`generation` понимают `.docx`, `.lotext`, `.loxlsx`;
  - `text_payload`/`read_text_payload`, `workbook_payload`/`read_workbook_payload`;
  - `Session::create`, `create_text`, `create_workbook`, `checkpoint*`,
    `restore*`, `discard`, `snapshots`, `discover`.
- `main.rs`:
  - `initialize_recovery`, `sync_recovery` (ветки text → workbook → docx),
  - `sync_text_recovery`, `sync_workbook_recovery`,
  - `restore_first_recovery` (диспетчер), `restore_first_text_recovery`,
    `restore_first_workbook_recovery`, `discard_first_recovery`,
  - `clear_current_recovery`, `sync_recovery` вызывается после кадра.
- Recovery-снапшот книги: листы, значения, `row_count`/`column_count`,
  форматирование (size/bold) по `CellKey`; лимиты 64 листа / 250 000 ячеек /
  32 МиБ.

## 6. Undo/Redo, поиск, замена

- `WorkbookSnapshot { sheets, formats }`, `workbook_snapshot`,
  `record_workbook_edit`, `apply_workbook_snapshot`, `undo_workbook`,
  `redo_workbook`.
- Снимок берётся до `show_grid`/`show_editor`, после изменения — push.
  Это даёт транзакцию на кадр; при наборе текста в ячейке история может быть
  посимвольной.
- `record_text_edit` вызывается после кадра; программные Replace/Undo/Redo
  ставят `suppress_text_history`, чтобы не дублировать записи.
- `search::Target::{DocxParagraph, DocParagraph, Cell, Text}`;
  `search::text` использует тот же лимит 10 000.
- `replace_docx`, `replace_spreadsheet`, `replace_text` — отдельные функции,
  каждая пишет ровно одну транзакцию истории.

## 7. Сборка и тесты

```powershell
$env:TMP = $env:TEMP = 'C:\DHW\.tmp'
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo build --release
```

- Release profile: `opt-level="z"`, fat LTO, `codegen-units=1`, `panic=abort`,
  strip.
- `cargo test --all-targets`: **239 passed, 9 ignored**.
  Разбивка на 26.10.2026: 121 lib, 83 bin, 3 `doc_export`, 1 `pdf_print`
  (+1 ignored), 2 `pdf_security` (+2 ignored), 4 `search_formats` (+3 ignored).
- Тесты ожидают **русские** строки статусов (глобальный язык по умолчанию RU).
  Не вызывай `set_english` в тестах и не запускай их параллельно с таким
  вызовом.
- `target/` не включён; первая release-сборка занимает ~50 с и несколько ГБ.

### Ignored-тесты

Проходят в этой сессии:

- `app_tests::local_legacy_doc_preview_image_regression` — на синтетическом
  `evidence/rc-manual/legacy-image-fixture.doc` (DOC + PNG): 1 изображение.
- `app_tests::windows_pdf_fixture_regression` — 3 PDF, поля/ориентация/номера,
  исходник не изменён.
- `export_and_verify_input_pdf_range_orientation_and_scale` — PDF range/scale.
- `render_existing_mixed_pdf_for_reader_qa` — PDF reader QA.

Не запускались (контракт «только новый каталог», существующие ручные
артефакты не перезаписываются):

- `export_manual_text_fixtures`, `export_manual_find_fixtures`,
  `export_manual_replace_fixtures`, `export_manual_workbook_fixtures`,
  `export_pdf_reader_evidence`.

## 8. Ручная регрессия

Подробно — `RC_MANUAL_REGRESSION_RU.md`. Кратко:

- DOCX print preview + системный диалог + отмена — PASS.
- GUI print-to-PDF — PASS по файлу `evidence/rc-manual/printer-user-print.pdf`
  (Microsoft Print To PDF, 2 страницы A4, кириллица, таблица, изображение,
  номера страниц).
- PDF view/print — PASS.
- Два окна на одном DOCX: первый save прошёл, второй получил
  «Документ изменён на диске», запись заблокирована, Cancel сохранил правки;
  в файле остался текст первого окна.
- Recovery после kill — PASS.
- Физический принтер — PASS по подтверждению пользователя; бумажного артефакта
  в комплекте нет.

## 9. Known Limitations (важно не выдавать за готовое)

- DOC/DOCX: неполная матрица форматирования; DOC→DOCX не переносит исходные
  стили/структуру таблиц.
- XLS/XLSX: числа, bool и даты импортируются и экспортируются типами;
  формулы не вычисляются, кеш не обновляется. ODS/XLSB — read-only.
- Лимиты/повреждённые файлы/ZIP bombs/ошибки записи покрыты частично.
- Legacy DOC: изображения — best-effort gallery из байтов контейнера; исходная
  вёрстка Word не воспроизводится. Синтетический fixture — именно про gallery.
- Печать: Word-вёрстка приблизительная; физический принтер проверен только
  пользовательским подтверждением. Системный предпросмотр Windows не даёт
  приложению задать свой масштаб; поля влияют на печать, не на вёрстку DOCX.
- Старые Phase 4 PARTIAL в docs/ — история апстрима. Финальный Windows-форк
  принят пользователем; актуальный статус определяет STATUS_RU.md.

## 10. Куда что править

- Новая строка UI → добавь пару в `tr!("ru","en")`/`tfm!` в том же модуле,
  где строка используется; проверь RU-тесты.
- Новая операция таблицы → оберни изменение в `workbook_snapshot` до и
  `record_workbook_edit` после; проверь Save As/recovery.
- Новый формат recovery → добавь `SessionKind`, suffix, payload/read, discovery,
  `restore_first_*`, очистку в save; тесты на round-trip и битую точку.
- Новый тип ячейки → `CellKind`/`classify` в `workbook_edit.rs`, style entry,
  XML-тег; тест на round-trip и на то, что ведущие нули остались текстом.
- Новая команда печати → `print.rs`/`pdf_print.rs`, не ломай защиту оригинала.
- Смена версии → `Cargo.toml`, `CHANGELOG`, `README_REDESIGN`,
  пересобери EXE/ZIP, обнови `.sha256` и `SHA256SUMS.txt`.

## 11. Грабли

1. **Длинные пути MSVC**: всегда `TMP/TEMP=C:\DHW\.tmp`.
2. **`target/` ~3 ГБ** — не клади в архив/комплект.
3. **Recovery lease-only каталоги** не удаляй у живого процесса; пустой lease
   после cleanup — норма, кандидатом не считается.
4. **Save работы с книгой** должен вызывать `clear_current_recovery` и сбрасывать
   `recovered_document`, иначе окно навсегда «грязное».
5. **`export_manual_*`** требуют нового каталога и падают, если он существует;
   это защита ручных файлов, а не баг.
6. **Нативные диалоги** автоматизируются плохо: окна перекрываются, нужен
   foreground/topmost; не делай вывод о дефекте приложения из неудачного клика.
7. **Язык в тестах** — глобальный; не переключай его в тестах.
8. **Типизация XLSX** намеренно консервативна: `true`/`false` в нижнем регистре
   остаются текстом, `007` — текстом.
9. **Оригинал** нельзя перезаписывать; любые новые save-пути должны сохранять
   это свойство.

## 12. Если продолжать после 1.0

Приоритеты (старый `docs/ROADMAP_TO_1_0_RU.md` — история апстрима):

1. Матрица форматирования DOC/DOCX и сохранение списков/таблиц (главный долг).
2. Стили/числовые форматы XLSX и инертные формулы (типы чисел/дат/bool уже
   импортируются).
3. Лимиты и устойчивость к повреждённым/большим файлам, ошибкам записи.
4. Собственный предпросмотр печати 100%, если нужен чёткий экранный preview
   вместо системного окна Windows.
5. Довести поля/проценты изображений до предсказуемого поведения или убрать
   неработающие настройки.

## 13. PDF-экспорт и типы XLSX

- «Файл → Экспорт PDF…» для DOCX и таблиц: `spreadsheet_print_blocks` строит
  страницы, `print::run_windows_print_helper(..., Some(path))` пишет PDF;
  на не-Windows — заглушка.
- XLSX-импорт: `parse_date_styles` + `excel_serial_to_iso` превращают числовые
  даты Excel в ISO; числа и bool уже сохраняются из XML (`t="n"`/`t="b"`).
- Формулы остаются инертными; кеш не пересчитывается.

## 14. Кроссплатформенность

- Windows — основная платформа, полный функционал.
- Linux — собран `x86_64-unknown-linux-musl` (static PIE), есть tar.gz и
  `.deb`; данные в `$XDG_DATA_HOME/notoffice` или `~/.local/share/notoffice`.
  Печать и просмотр PDF отключены (Windows-only заглушки).
- macOS — `cargo check --target x86_64-apple-darwin` проходит; готового `.app`
  нет, сборка на Mac/CI через `packaging/macos/build-macos.sh`.
- Подробности и статус проверок — `PORTABILITY_RU.md`.
- `app_paths.rs` нельзя заменять на `current_exe().parent()` для Linux/macOS:
  установленный `.deb`/`.app` не должен писать в `/usr/bin` или `/Applications`.

## 15. Лицензии

- Код проекта — MIT (`LICENSE`).
- Все зависимости в дереве Windows/Linux/macOS — permissive; GPL/AGPL/SSPL/
  unknown не найдены (проверено `cargo tree` + `cargo metadata`).
- Встроенные шрифты egui/epaint: Ubuntu-Light (UFL), NotoEmoji/Hack (OFL/MIT),
  emoji-icon-font (MIT); тексты — `licenses/`.
- Системные шрифты Windows, Print to PDF, PowerShell/.NET не распространяются.
- Полный аудит — `THIRD_PARTY_LICENSES.md`; при добавлении зависимости
  проверять её лицензию и обновлять этот файл.

## 16. Формат проекта (согласовано)

`PROJECT_LAYOUT_RU.md` фиксирует формат, который пользователь хочет видеть:
корень `C:\NotOffice` = одна актуальная папка версии + её EXE + юзергайд;
внутри папки — исходники, тесты, docs, evidence, screenshots, архивы,
SHA256SUMS, handoff/юзергайд/CHANGELOG. Никаких `target/`, runtime-файлов,
старых версий и `releases/`. Runtime (`NotOffice-recovery`, recent/settings/
language) создаётся рядом с запущенным EXE и в папку версии не входит.
При следующих изменениях сохранять эту структуру; отклонения — только по
запросу пользователя.

## 17. Контрольные суммы

Хэши EXE/ZIP — в соседних `*.sha256`; `SHA256SUMS.txt` — по всем файлам
комплекта. После пересборки обновлять оба уровня.
