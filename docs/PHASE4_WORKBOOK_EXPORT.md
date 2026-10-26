# Phase 4 — XLS/XLSX values copies, rc.20

## Latest manual acceptance — user-reported PASS

User confirmed the rc.20 short checklist succeeded: direct cell input,
Tab/Shift+Tab and Enter/Shift+Enter navigation, Ctrl+S saving the copy. This
supersedes NOT VERIFIED for that native workflow only. No independent artifact
inspection is claimed for this confirmation. Full workbook compatibility,
crash recovery, replacement and the deferred regression remain open.

## Current change — direct cell input

User screenshots establish that rc.9's upper cell-text field does work. The
request is to edit directly in the document. rc.20 replaces that field with an
inline editor: click a cell and type, including blank cells. Tab/Shift+Tab moves
columns, Enter/Shift+Enter moves rows. Each selected cell retains an independent
widget ID; programmatic focus is one-shot and does not steal Find/toolbar focus.
The upper row now contains only cell reference/font controls. The original
protected-copy conversion remains explicit; limits below remain applicable.

Automated full UI PASS: mouse press/release on blank B1, focus, Cyrillic text,
Tab/Shift+Tab/Enter/Shift+Enter, saved XLSX/reopen and unchanged original. Grid
sizing passes keep pending focus until the widget becomes interactive. Black
text/caret and white editor background are explicit. Native rc.20 GUI behavior
remains NOT VERIFIED; no deferred printer/recovery manual regression was run.

## Earlier report, clarified by screenshots

User initially reported unavailable typing/caret. Subsequent screenshots show
the upper field containing an edit and the grid reflecting it. The first
screenshot is read-only after reopen; the second is the editable copy. Do not
claim an independently reproduced rc.9 keyboard defect from this report.
The separate rc.9 native QA attempt did not establish input behavior: automated
actions did not reliably affect even the initial blank window. No production
code or root release was changed during this diagnosis.

Implemented: explicit values-copy editor for XLS/XLSX, all imported sheets,
cell selection and literal text input, new blank cells, integer font size 6–72
and bold. XLSX reads these supported font properties back for display/export.
Save As creates a real OPC XLSX with workbook/sheets/styles/content types;
subsequent Ctrl+S uses the exported destination, an atomic replacement and
separate .xlsx.bak backups. Both origin paths and hard-link aliases are protected.
External changes to the adopted destination block overwrite and keep edits.
Existing unsaved-close/open and conflict UI now handles workbook copies too.
Cell editor IDs isolate widget Undo state by document generation, sheet and cell.

## Limits — Phase 4 and FR-02 remain PARTIAL

- Every exported value is literal text, including numbers/dates, formula caches
  and user-entered text starting with '='. No formulas are evaluated or created.
- Original formulas, charts, images, tables, merges, links, number formats,
  original layout and unsupported styles are not retained. Disclosure appears
  before conversion; the original workbook is never rewritten by this conversion.
- XLS font styles are not imported. XLSX supports integer size/bold only.
- New generated packages contain no copied VBA/script/OLE/external-link parts.
- Workbook crash recovery is NOT IMPLEMENTED; the UI says to save regularly.
- Workbook Replace current/all and model-wide Undo/Redo are not implemented.
  Native text-field undo is per cell; DOCX replacement/undo remains unchanged.
- Export limits: 64 sheets, 250000 cells, Excel coordinate/text limits,
  32 MiB input text / 48 MiB serialized XML. Invalid data fails before replacement.

## Evidence

Automated PASS: XLS and XLSX import -> edit -> XLSX -> reopen all sheets; literal
formula text; Cyrillic/XML escaping; empty cell/sheet; supported font round-trip;
independent Calamine cross-reader; protected source/hard-link; failed write,
backup, external-change conflict; two-window state; close guard; full egui UI
focus/input; distinct per-cell undo IDs. No real-printer/recovery regression run.

Native Windows GUI workflow and opening the result in Excel: NOT VERIFIED.
Do not relabel the headless UI or independent library check as native acceptance.

Fixtures: `manual_phase4_artifacts/workbook-rc9/workbook-test.xls` and
`workbook-test.xlsx`. Synthetic Main/Notes sheets include a distant AD500 cell;
the XLS formula has a deliberately different cached value, to detect execution.
No permanent exported result is claimed before the manual workflow is performed.

## Short manual check, for each input format

1. Open the fixture; choose Edit values as XLSX copy. Click A1 and type directly
   in the grid; change font to 16 and Bold. Edit another sheet too. Check Tab and
   Enter, and their reverse Shift combinations, between cells.
2. Ctrl+S -> choose a new XLSX. Reopen it; verify both sheets, text and fonts.
   Perform another edit/save and verify .xlsx.bak. Original input stays unchanged.
3. Close with edits: Cancel keeps them; Save/Save As produces the edited copy.
4. Two windows: use the exported XLSX as input in both, explicitly start editing,
   and Save As both to a second shared XLSX. After both sessions adopt that path,
   change/save in the first, then change/save in the second: conflict must block
   overwrite. Cancel preserves edits; Save a copy writes a distinct file.

The shared target in step 4 must be different from both protected inputs; this
candidate's editor deliberately works on copies rather than original workbooks.
