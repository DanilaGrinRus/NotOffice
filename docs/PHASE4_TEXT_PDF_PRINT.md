# Phase 4 — rc.21: blank TXT, basic MD/RTF, input-PDF printing

Latest acceptance: user reports all three short rc.21 checklist items passed
(immediate blank TXT typing/save/reopen, basic MD/RTF edit/Save As/reopen, PDF
range/scale/printer workflow and cancellation). This is user-reported PASS for
that short set, superseding its earlier NOT VERIFIED. New physical-printer error
and full multi-window/deferred regression remain NOT VERIFIED. No independent
inspection of additional user output files is claimed.

## Implemented and limits

- New application / Ctrl+N window starts with an editable Untitled.txt and
  one-shot focus; no click/initialization required. Multiline editing, Ctrl+S,
  Save As, dirty close/open guards, backups and external-change conflict.
- TXT/MD: UTF-8 and BOM UTF-16 input, UTF-8 output; Markdown is literal source,
  without rendered HTML, automatic links or code execution.
- Basic RTF: passive text extraction (CP1251/1252, Unicode including surrogate
  pairs, paragraphs/tabs). Formatting, tables, pictures, headers, fields and
  objects are not preserved. The initially opened RTF is protected; Save As
  creates a separate text-only RTF/TXT/MD. No OLE/RichEdit/Office scripting.
- Text input/serialized files limited to 8 MiB. New text formats have no
  application Find/Replace or crash recovery yet; UI discloses recovery limit.
- PDF Print/Ctrl+P opens a modal range and Fit/10–400% dialog, then the Windows
  printer selector. Source-page orientation is automatic. Only selected pages
  are rasterized; max dimension 2000 px, total job rasters 256 MiB. Source PDF
  bytes are snapshotted at Open, so print does not reread a replaced path.
  No vector/text-layer preservation is claimed. Actual-size/custom content can
  exceed printable area; Fit is the default. No PDF actions/links are dispatched.

## Automated evidence

- Full UI test: first text event in a fresh window, retained focus, save and
  close guard. Text format switches and failed Open retain the active document.
- Text tests: Unicode/emoji/RTF escapes/CP1251; literal MD; ignored object and
  instruction destinations; invalid/binary/oversized nesting rejection; safe
  Save As/backups, original RTF/alias protection and same-file conflict.
- PDF range/scale validation, passive immutable-snapshot printing and existing
  native no-Launch/no-OpenAction tests. Ordinary suite never opens print dialogs.
- Explicit new Microsoft Print to PDF test: select only landscape page 2 from
  a two-page source (blue page 1/red page 2); output one landscape page, only red
  content. Fit vs 50% output red widths: 1599 vs 341 px. Both centered within 3%
  of the page center; source bytes unchanged. Final PNGs visually inspected.
- Initial output exposed portrait PrintableArea on a landscape page. Fixed
  driver-area normalization; stronger image-center assertions pass afterward.
- Earlier miscentered diagnostics remain in pdf-print-rc11/ as failed evidence;
  use only pdf-print-rc11-final/ for the verified outputs below.

## Artifacts

`manual_phase4_artifacts/text-rc11/`: text-test.txt, markdown-test.md, rtf-test.rtf.
`manual_phase4_artifacts/pdf-print-rc11-final/`: source.pdf, page-2-fit.pdf,
page-2-50pct.pdf and corresponding rendered PNGs.

## Native acceptance still NOT VERIFIED

Real-window blank typing / MD/RTF edits, system printer selection and cancellation,
unavailable physical printer, and independent-window end-to-end workflows need
manual confirmation. Driver output tests and headless egui tests do not waive
these checks. The user-deferred old printing/recovery regression was not run.
Phase 4 and mandatory cross-format gates remain PARTIAL.

Short manual checklist:
1. Start the rc.21 EXE without a file; type immediately, Ctrl+S to a new TXT,
   reopen it. Close with another edit: Cancel keeps the text.
2. Open each text fixture, edit and Save As to a new file, reopen. RTF text is
   retained; formatting is deliberately unsupported. MD markup stays literal.
3. Open source.pdf, Ctrl+P, select 2–2, Fit, select Microsoft Print to PDF and
   save to a NEW name. Expect one landscape page with the centered red shape.
   Repeat at 50%; shape becomes smaller. Cancellation must leave PDF unchanged.
4. In two independent text windows edit the same TXT: after one saves, the second
   must block overwrite and preserve edits; Save a copy writes another file.
