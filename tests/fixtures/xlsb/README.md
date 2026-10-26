# XLSB regression fixtures

`any_sheets.xlsb`, `date.xlsb`, and `issues.xlsb` are upstream calamine test
fixtures: https://github.com/tafia/calamine/tree/master/tests (MIT license,
included as LICENSE-MIT.md). Copied from the files used in XLSB_CHECK_RU.md.

`issues-cyrillic.xlsb` is the existing test derivative: the shared string `test`
is replaced by `тест` in UTF-16LE. No scripts or macros are executed by tests.

Mixed workbook: three worksheets (Visible, Hidden, VeryHidden) and a Chart
sheet. Only worksheets have cell ranges. Dates: 2021-01-01, 2021-01-02 and
the elapsed time 255:10:10. The issues fixture includes 2016-10-20.
