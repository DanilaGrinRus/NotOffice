# Формат проекта NotOffice

Зафиксировано 26.10.2026 для Windows 1.0.0. Это **согласованный формат**:
он примерно повторяет текущую структуру и должен сохраняться в дальнейшей
работе. Любое отклонение — только по отдельному запросу пользователя.

## Целевое дерево

```
C:\NotOffice\
├── NotOffice-<version>\        # единственная актуальная версия
│   ├── src\                             # исходники
│   ├── tests\                           # тесты и fixtures
│   ├── docs\                            # документация апстрима/история
│   ├── evidence\                        # доказательства проверок
│   │   ├── print-pdf\
│   │   ├── pdf-print\
│   │   ├── rc-manual\
│   │   └── test-logs\
│   ├── screenshots\
│   ├── packaging\                      # скрипты Linux/macOS/Windows и CI
│   ├── packages\linux\                 # Linux tar.gz и .deb
│   ├── LICENSE, THIRD_PARTY_LICENSES.md
│   ├── licenses\                      # тексты лицензий и шрифтов
│   ├── Cargo.toml, Cargo.lock, .gitignore
│   ├── README_RELEASE_RU.md
│   ├── GPT_HANDOFF_RU.md
│   ├── PROJECT_LAYOUT_RU.md
│   ├── PORTABILITY_RU.md
│   ├── USER_GUIDE_RU.md
│   ├── CHANGELOG_<version>_RU.md
│   ├── RECOVERY_CHECK_RU.md
│   ├── RC_MANUAL_REGRESSION_RU.md
│   ├── NotOffice-<version>.exe
│   ├── NotOffice-<version>-portable.zip (+ .sha256)
│   ├── NotOffice-<version>-sources.zip (+ .sha256)
│   └── SHA256SUMS.txt
├── NotOffice-<version>.exe     # быстрый запуск
└── USER_GUIDE_RU.md                     # быстрый доступ к инструкции
```

## Правила

1. **Одна актуальная версия.** В корне проекта — только текущая папка версии,
   её EXE и юзергайд. Принятый RC сохранён для отката в C:\NotOffice\accepted-rc2-backup
   вне поставки. Кэши аудита удалены по запросу пользователя; runtime-файлы сохранены.
2. **Папка версии самодостаточна.** Внутри есть всё, чтобы работать без
   интернета: исходники, `Cargo.toml/lock`, тесты, документация, evidence,
   архивы, контрольные суммы. Копирование одной папки должно сохранять проект.
3. **Никакого build-мусора.** В папке версии не должно быть `target/`,
   `target-static/`, `tmp/` и старых `manual_phase*` — только отобранные
   `evidence/`.
4. **Runtime — отдельно от релиза.** `NotOffice-recovery\`,
   `NotOffice-recent.txt`, `NotOffice-settings.txt`,
   `NotOffice-language.txt` создаются рядом с запущенным EXE. В папке версии
   их быть не должно; запускать удобнее корневой EXE, тогда runtime остаётся
   в корне проекта.
5. **Документация разделена.** Пользовательские документы (README релиза,
   GPT-handoff, layout, юзергайд, CHANGELOG, протоколы) — в корне папки
   версии. История апстрима, roadmap, Phase 4, требования — в `docs/`.
6. **Evidence — по типам проверок.** Скриншоты/PDF/логи не сваливаются в
   корень: `evidence/print-pdf`, `evidence/pdf-print`, `evidence/rc-manual`,
   `evidence/test-logs`.
7. **Хэши — обязательны.** `SHA256SUMS.txt` покрывает все файлы папки версии;
   корневой EXE должен совпадать с EXE внутри папки. После любого изменения
   файлов хэши пересчитываются, архивы пересобираются.
8. **Архивы соответствуют папке.** `portable.zip` — только необходимое для
   работы: EXE, `NotOffice-1.0.0.exe.sha256`, `USER_GUIDE_RU.md`, `LICENSE`,
   `THIRD_PARTY_LICENSES.md` и `licenses/`. Документация, скриншоты и evidence остаются в папке версии.
   `sources.zip` — исходники, тесты, документация, evidence.
9. **Версия — в одном месте.** `Cargo.toml` → `version`; имя папки, EXE,
   архивов и документов использует ту же версию. Старые имена не остаются.
10. **Сначала тесты, потом упаковка.** `fmt`, `check`, `test`, `clippy`,
    `build --release`; затем обновление EXE/архивов/хэшей и краткий отчёт.

## Что считается «лишним»

- `target/`, `target-static/`, `tmp/`, кэши сборки;
- старые EXE/ZIP и `releases/`;
- runtime-файлы и recovery-сессии;
- дубли документов в корне и в `docs/`;
- личные/временные файлы, не относящиеся к релизу.

## Текущее состояние

- Корень: `C:\NotOffice`.
- Версия: `1.0.0`.
- В корне: папка версии, `NotOffice-1.0.0.exe`,
  `USER_GUIDE_RU.md`.
- Runtime-файлы пользователя в корне сохраняются; в архивы выпуска они
  не включаются. Временные файлы сборки удалены; резервный RC остаётся вне поставки.
