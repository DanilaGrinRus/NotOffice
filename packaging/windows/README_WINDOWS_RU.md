# Windows

Готовая сборка: `NotOffice-1.0.2.exe` в корне проекта.
Портативная: настройки/recovery создаются рядом с EXE, поэтому запускай из
папки с правом записи.

Сборка:

```powershell
$env:TMP = $env:TEMP = 'C:\DHW\.tmp'
cargo build --release
```

Артефакт: `target\release\notoffice.exe`.
