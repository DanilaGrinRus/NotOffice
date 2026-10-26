# Сторонние лицензии NotOffice 1.0.0

Этот файл — не юридическое заключение, а инвентаризация того, что реально
попадает в сборку и что нужно приложить при распространении.

## Краткий вывод

- Код NotOffice — **MIT** (`LICENSE`).
- Прямые и транзитивные Rust-зависимости — **permissive**: MIT, Apache-2.0,
  BSD-2/3, ISC, Zlib, BSL-1.0, Unicode-3.0, Unlicense/CC0, OFL/UFL для шрифтов.
- **GPL, AGPL, SSPL, LGPL-only, closed-source или unknown-лицензий в дереве
  сборки не найдено.** Проверка: `cargo tree` для Windows/Linux/macOS +
  `license`-поля из `cargo metadata`; подозрительных совпадений 0.
- `r-efi` имеет тройную лицензию `MIT OR Apache-2.0 OR LGPL-2.1-or-later`;
  используется permissive-вариант, LGPL-обязательств нет.
- **Системные шрифты Windows не распространяются**: приложение читает
  `C:\Windows\Fonts\segoeui.ttf`, `arial.ttf`, `seguisb.ttf` во время работы.
  В комплект они не входят.
- **Microsoft Print to PDF / PowerShell / .NET** — компоненты Windows,
  вызываются как системные, не входят в архив.
- В бинарник встроены шрифты egui/epaint: Ubuntu-Light (UFL-1.0),
  NotoEmoji и Hack (OFL-1.1 / MIT-части), emoji-icon-font (MIT).
  Их тексты — в `licenses/`.

## Прямые зависимости

| Крейт | Лицензия | Примечание |
|---|---|---|
| `eframe` | MIT OR Apache-2.0 | GUI |
| `rfd` | MIT | системные диалоги |
| `quick-xml` | MIT | XML |
| `zip` | MIT | ZIP |
| `calamine` | MIT | XLS/XLSX/XLSB/ODS |
| `cfb` | MIT | legacy DOC |
| `encoding_rs` | (Apache-2.0 OR MIT) AND BSD-3-Clause | кодировки |
| `image` | MIT OR Apache-2.0 | JPEG/PNG |
| `windows`, `windows-sys` | MIT OR Apache-2.0 | Windows API |
| `tempfile` | MIT OR Apache-2.0 | только dev/tests |

## Транзитивные зависимости

Дерево `cargo tree` на 25.10.2026:

| Платформа | Узлов | Копилефт/unknown |
|---|---:|---:|
| Windows x86_64 | ~300 | 0 |
| Linux x86_64 | ~741 | 0 |
| macOS x86_64 | ~321 | 0 |

Встречающиеся лицензии: MIT, Apache-2.0, BSD-2/3, ISC, Zlib, BSL-1.0,
Unicode-3.0, Unlicense, CC0, OFL-1.1, UFL-1.0. Тексты основных лицензий
лежат в `licenses/`.

`Unicode-3.0` (крейты ICU/unicode) — permissive; полный текст:
<https://www.unicode.org/license.txt>.

`BSL-1.0` (`clipboard-win`, `error-code`) требует сохранять уведомление;
текст — `licenses/BSL-1.0.txt`.

## Шрифты

| Шрифт | Проект | Лицензия | Файл |
|---|---|---|---|
| Ubuntu-Light | Canonical | Ubuntu Font Licence 1.0 | `licenses/UFL-1.0.txt` |
| NotoEmoji-Regular | Google | SIL OFL 1.1 | `licenses/OFL-1.1.txt` |
| Hack-Regular | Source Foundry / DejaVu / Bitstream Vera | MIT + public domain + Bitstream Vera | `licenses/Hack-Regular.txt` |
| emoji-icon-font | egui project | MIT | `licenses/emoji-icon-font-MIT.txt` |

Эти шрифты встроены в `epaint 0.28` (feature `default_fonts`) и попадают в
Windows/Linux/macOS-бинарники. При распространении сохраняй `licenses/`.

## Системные компоненты

- **Windows:** сегменты `Microsoft Print to PDF`, `.NET`/PowerShell-helper,
  системные шрифты. Не входят в комплект; регулируются лицензией Windows.
- **Linux .deb:** зависимости `libx11-6`, `libxkbcommon0`, `libwayland-client0`,
  `libgl1`, `libfontconfig1` и др. — системные библиотеки Debian/Ubuntu,
  не входят в пакет.
- **macOS:** системные фреймворки Apple; `.app` не собран в этой сессии.

## Что сделать перед публичным релизом

1. Приложить `LICENSE`, `THIRD_PARTY_LICENSES.md` и папку `licenses/` к
   каждому архиву/установщику (уже включены в `portable.zip`/`sources.zip`).
2. Проверить имя **NotOffice** на товарный знак в юрисдикции публикации —
   это не вопрос лицензии, но риск бренда.
3. Для macOS-подписи/нотаризации и Windows Authenticode — отдельные
   сертификаты, не входят в проект.
4. Если добавляешь новую зависимость — прогоняй `cargo tree` и проверяй
   `license`; не тяни GPL/AGPL в статическую сборку без юридического решения.
