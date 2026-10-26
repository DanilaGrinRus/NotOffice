# LiteOffice release progress

## Current checkpoint — Phase 4 PARTIAL, 0.8.0-rc.22, 2026-10-25

Added TXT/MD/RTF crash recovery, including Untitled.txt. Reuses exclusive OS-lock
session ownership and the existing startup recovery dialog. New immutable .lotext
generations store UTF-8 text/type, byte length and checksum, with sync/validation
before publishing. Keeps a valid predecessor and falls back on corrupt newest
data; invalid-only recovery is retained. Active/claimed sessions are excluded.
Restored text stays passive; RTF-like strings are not reinterpreted as objects
or instructions. Restore requires Save As copy and protects original paths and
aliases, including rejecting exports inside recovery storage. Save/explicit
discard retires the session; cancel/failed saves keep it. Per-window checkpoints
are attempted every 2 seconds with explicit pending/saved/write-failure status.

Confirmed and fixed during acceptance: clean close after undoing every text edit
left an obsolete checkpoint offered on restart. New failing test reproduced it;
clean text close now retires recovery. Actual crashes still retain checkpoints.

Final source gates PASS: cargo fmt --check; cargo check --all-targets --offline;
cargo test --all-targets --offline (**194 passed / 9 ignored**);
cargo clippy --all-targets --offline -- -D warnings; cargo build --release
--offline; git diff --check. Test worker was forcibly killed while owning four
text sessions (untitled/TXT/MD/RTF): locks released, all four recovered with correct
text/type and protected originals. Additional tests cover checksum corruption,
fallback/rotation, failed write, empty text, window isolation, Save As/cleanup,
cancel/Don't save and restored egui focus. Existing DOCX tests also pass.

Native GUI crash/restart acceptance: NOT VERIFIED; automated child-process and
headless UI evidence is distinguished from real-window interaction. No old
ignored printing/recovery manual regression was run. Workbook recovery remains
unimplemented. Phase 4 remains PARTIAL. Manual steps: PHASE4_TEXT_RECOVERY.md.
Existing manual text fixtures reused from manual_phase4_artifacts/text-rc11/;
new automated sources/snapshots are temporary, no permanent DOCX/PDF created.

Root EXE: C:\NotOffice\LiteOffice-v0.8.0-rc.22.exe, 4,301,312 bytes;
SHA-256 7f69c2c3a206eb4bf9be6840d190287b6cf85839eae9f80bd142c14159aad99e,
matching target/release; SHA sidecar added. The unreleased candidate was refreshed
after the clean-close fix; all earlier accepted/candidate versions preserved.
Branch release/stabilization, HEAD 44a651a, dirty tree; no staging/commit/tag.
This increment: Cargo.toml/lock, src/recovery.rs, src/text_document.rs, src/main.rs,
README/master plan/roadmap/mandatory requirements/this log; new
PHASE4_TEXT_RECOVERY.md and root EXE/hash. Other dirty work and user files preserved.

## Current checkpoint — Phase 4 PARTIAL, 0.8.0-rc.21, 2026-10-25

User acceptance update: short rc.21 blank TXT / basic MD-RTF / PDF-print checklist
reported PASS. This supersedes native NOT VERIFIED for that set only; physical
printer errors, full multiple-window acceptance and deferred regression remain
open. Current roadmap written in ROADMAP_TO_1_0_RU.md; historical roadmap retained
with a pointer to current ordering. This roadmap update changes documentation
only; no new EXE or source verification is claimed.

New windows start with a focused, editable Untitled.txt. Added literal TXT/MD
and basic text-only RTF loading/editing/saving, safe writes/backups and conflict
protection. RTF object/picture/instruction destinations are skipped; no document
scripts are executed. Original imported RTF is protected until Save As. New text
formats have no Find/Replace or crash recovery yet; UI/docs disclose limitations.

Input-PDF Print/Ctrl+P now provides page range, Fit/10–400% scale, then system
printer selection. Immutable opened bytes -> passive native PDF rasters ->
trusted bitmap-only print helper. Source orientation selected per page; images
limited to 2000 px/side and 256 MiB/job. No vector preservation is claimed.
Printing does not reread the source path or execute actions/JavaScript/attachments.

Confirmed/fixed PDF defect: the PDF printer driver reported portrait printable
area for a landscape page. Initial generated output was visibly off-center.
Normalized area orientation; final checks enforce centering within 3% on both
axes. Explicit NEW Microsoft Print to PDF check PASS: page 2 only -> one landscape
page with red marker, no blue page-1 content. Fit/50% marker widths 1599/341 px,
both centered. Original source bytes unchanged. Final PNGs visually inspected.
Failed initial diagnostics retained separately, not presented as passing output.

Final source gates PASS: cargo fmt --check; cargo check --all-targets --offline;
cargo test --all-targets --offline (**187 passed / 9 ignored**);
cargo clippy --all-targets --offline -- -D warnings; cargo build --release
--offline; git diff --check. Additional explicitly invoked fixture exporters:
text fixtures PASS; new PDF print-to-PDF check PASS. Old ignored printing/recovery
regression was not run. Headless tests cover first-frame blank typing, retained
focus, save/close guards and text format switches, Unicode/emoji/CP1251 RTF,
passive object/instruction skipping, failed reads, backups and save conflict.

Native GUI blank typing/MD/RTF edits, interactive printer selection/cancellation,
unavailable physical printer and full multiple-window acceptance: NOT VERIFIED.
Driver automation and headless UI tests do not establish those manual outcomes.
Phase 4 remains PARTIAL. Details/checklist: PHASE4_TEXT_PDF_PRINT.md.

Artifacts: manual_phase4_artifacts/text-rc11/{text-test.txt,markdown-test.md,
rtf-test.rtf}; verified PDF outputs in manual_phase4_artifacts/pdf-print-rc11-final/
(source.pdf, page-2-fit.pdf, page-2-50pct.pdf and PNGs). Earlier pdf-print-rc11/
contains pre-fix diagnostic outputs only.

Root EXE: C:\NotOffice\LiteOffice-v0.8.0-rc.21.exe, 4,293,120 bytes;
SHA-256 ff294167ab169baeaf01206a4fd16c5751ff98a9a61bf9088322e9b2c4ba0f9d,
matching target/release. Previous candidates and accepted v0.7.5 preserved.
Branch release/stabilization, HEAD 44a651a; dirty tree, no staging/commit/tag.
This increment: Cargo.toml/lock, src/main.rs, src/file_ops.rs, src/lib.rs,
src/pdf.rs, src/pdf_view.rs, src/print.rs; new src/text_document.rs,
src/pdf_print.rs, src/pdf_print_windows.ps1, tests/pdf_print.rs,
PHASE4_TEXT_PDF_PRINT.md; README/master plan/security/mandatory requirements and
this log updated. New root EXE/hash and fixture folders. Other dirty work/user
files preserved; no cleanup or old-phase regression performed.

## Current checkpoint — Phase 4 PARTIAL, 0.8.0-rc.20, 2026-10-25

Acceptance update: user reports the rc.20 short native checklist all passed
(inline cell input, four Tab/Enter navigation combinations, Ctrl+S saving).
Mark this workflow user-reported PASS, superseding its earlier NOT VERIFIED.
No independent new artifact inspection or broader phase acceptance is implied.

User screenshots clarified the rc.9 report: the upper text field was working,
but direct editing in the spreadsheet was requested. Replaced that mandatory
upper field with an inline editor in the active cell. One click selects/focuses
a filled or blank cell; type directly in the grid. Tab/Shift+Tab move columns,
Enter/Shift+Enter move rows, with preview bounds including newly reached cells.
Font controls remain above; source protection/explicit values-copy conversion,
safe XLSX export and previously documented format/recovery limits remain.
Caret/text colors and white editor background are explicit. Grid's disabled
initial sizing pass retains pending focus until its interactive pass.

Final gates PASS (offline Cargo): fmt, all-targets check, all-targets test
**178 passed / 7 ignored**, Clippy with warnings denied, release build,
git diff --check. Full egui mouse/key test selects blank B1 by press/release,
types Cyrillic, traverses cells with all four key combinations, saves/reopens
XLSX and verifies original bytes. Existing focus and per-cell ID tests adapted
to grid sizing. No ignored print/recovery regression was executed.

Native rc.20 GUI acceptance: NOT VERIFIED. Prior native automation diagnosis
did not reliably control the isolated empty rc.9 window; do not count it as
reproduction of a keyboard defect or proof of the rc.20 behavior.
User screenshots establish rc.9 field input, not rc.20 inline acceptance.

Root: C:\NotOffice\LiteOffice-v0.8.0-rc.20.exe, 4,262,912 bytes;
SHA-256 63266ba593e380d6480b06c7bd2a651370f835c911679fd644f010e4b2aa1729,
matching target/release. Root SHA sidecar added; old builds preserved.
This increment: src/spreadsheet_ui.rs, src/main.rs, Cargo.toml/lock, README,
master plan, PHASE4_WORKBOOK_EXPORT.md and this log; root EXE/hash. Diagnostic
isolated rc.9 folder added in manual_phase4_artifacts/workbook-input-rc9.
Existing dirty files/user artifacts preserved; no commit/tag/staging.

## Current checkpoint — Phase 4 PARTIAL, 0.8.0-rc.9, 2026-10-25

Added explicit XLS/XLSX values-copy editing and real XLSX export. All imported
sheets, literal cell text, blank-cell insertion, integer font size 6–72 pt and
bold are supported. XLSX reads back these supported fonts; cell input receives
one-shot focus and has per-document/sheet/cell widget IDs. Original workbooks
and their hard-link aliases are protected. Save As adopts the new destination;
later Ctrl+S uses atomic replacement, .xlsx.bak and external-change detection.
Unsaved close/open and conflict dialogs handle workbook copies. Shared atomic
file helpers now select backup extension from the destination (DOCX unchanged).

Scope is explicitly values-only: numbers/dates/formula caches become literal
text; formulas, charts, tables, images, merges, number formats, layout and other
styles are not retained. UI warns before conversion. No VBA/scripts/links are
copied or formulas evaluated. Workbook crash recovery, full model Undo/Redo and
Replace current/all are NOT IMPLEMENTED; original formatting fidelity remains
open. Do not close FR-02 or Phase 4 from this increment.

Automated gates PASS: cargo fmt --check; cargo check --all-targets --offline;
cargo test --all-targets --offline (**177 passed, 7 ignored**);
cargo clippy --all-targets --offline -- -D warnings; cargo build --release
--offline; git diff --check. New tests cover both source formats/all sheets,
font/value round-trip, Calamine cross-read, empty cell/sheet, formula text,
protected paths/aliases, write failure, backup/conflict, independent window
state, close guard and full headless UI input/focus. A new replacement test
caught Windows locking the still-open temporary file; fixed by closing it before
verification/replacement, then final gates reran successfully.

Native GUI and Excel cross-open: NOT VERIFIED. No deferred manual printing or
recovery regression was run. One ignored synthetic workbook-fixture export was
explicitly run (PASS), without invoking the other ignored checks.
Fixtures: manual_phase4_artifacts/workbook-rc9/workbook-test.xls
(SHA-256 df05ceed606f74be8f2195b6617f1067a6eeb53bc70ad53a3392406651c15252)
and workbook-test.xlsx
(SHA-256 86d7dcb083cf000c536477db5672cc2e8921deca2a14f0c1629fa104b71e56b5).
Details/manual steps: PHASE4_WORKBOOK_EXPORT.md.

Root executable: C:\NotOffice\LiteOffice-v0.8.0-rc.9.exe, 4,260,864 bytes;
SHA-256 40093af04efed25f60ae49f4fb98a247eaab30dc4d6aabdaedbe6497800b387b,
matching target/release. SHA sidecar added; v0.7.5 and older candidates preserved.
Branch release/stabilization, HEAD 44a651a, dirty worktree; no staging/commit/tag.
This increment: Cargo.toml/lock, src/docx.rs (shared file helpers), src/main.rs,
src/lib.rs, src/xlsx.rs, src/spreadsheet_ui.rs, tests/search_formats.rs;
new src/workbook_edit.rs, PHASE4_WORKBOOK_EXPORT.md, root executable/hash and
synthetic fixture folder. README, master plan, mandatory requirements and this
log updated. Other pre-existing changes and user files preserved.

## Current checkpoint — Phase 4 PARTIAL, 0.8.0-rc.8, 2026-10-25

Manual acceptance update: user reports the short DOC conversion/edit/font-size/
Save As/reopen scenario PASS with three screenshots. Independently inspected
`C:\NotOffice\export-test-copy.docx`: edited text retained, both runs 16 pt
(`w:sz=32`). Original QA DOC SHA-256 unchanged. This supersedes the earlier
native end-to-end NOT VERIFIED for this scenario only. Full Phase 4 remains
PARTIAL; source style/table fidelity and external Word cross-open remain open.

DOC preview now offers explicit Edit as DOCX conversion. The copy supports the
existing text editor, font-size formatting, Find/Replace, Undo/Redo, dirty/close
guards, recovery and safe DOCX Save As. Original DOC styles/table structure are
not imported; this is disclosed before conversion. Known/manual image anchors
are retained as paragraph-relative placements; unplaced pictures go at the end.
Legacy streams/macros/external relationships are not copied to the new package.
The original DOC cannot be saved over, including hard-link aliases; comparison
now checks file identity rather than only canonical pathname. Corrupt recovered
images block conversion rather than silently disappearing.

Final automated gates PASS (offline Cargo): fmt, all-targets check, all-targets
tests **172 passed / 6 ignored**, Clippy with warnings denied, release build,
git diff --check (line-ending notices only). Initial restricted test invocation
failed on Windows file-operation permissions; unrestricted rerun passed. A new
hard-link alias test initially failed and was fixed before the final gates.

Native GUI end-to-end conversion/typing/Save As/reopen: NOT VERIFIED. Isolated
rc.8 window launched, but automation was interrupted by detected user input
before conversion. No user document was edited through that window. External
Word cross-open NOT VERIFIED. Deferred full manual regression not run.
QA input: `manual_phase4_artifacts/doc-export-rc8/export-test.doc` (synthetic
copy), SHA-256 `fc56558c7527ec184661064945330a29bfb9913b795b5c56c90256d9c21ce92a`.
Export tests use temporary synthetic CFB/DOCX files; no permanent QA DOCX or PDF
was produced this increment. See PHASE4_DOC_EXPORT.md for support and recheck.

Root: `C:\NotOffice\LiteOffice-v0.8.0-rc.8.exe`, 4,225,536 bytes;
SHA-256 `2b92c78e7bad8fdec3181c4de7a7c61f2e922425447a6e816a026dd944eacdb3`.
Hash equals target/release executable; accepted v0.7.5 and old candidates kept.
Branch release/stabilization, HEAD 44a651a; dirty tree, no commit/tag/staging.
This increment: Cargo.toml/lock, src/docx.rs, src/main.rs, src/lib.rs;
new src/doc_export.rs, tests/doc_export.rs, PHASE4_DOC_EXPORT.md; updated README,
master plan, mandatory requirements and progress. Added isolated QA artifacts
and root EXE/hash. All other pre-existing changes preserved.

FR-02 remains PARTIAL: original DOC styling/table fidelity and editable XLS/XLSX
export are still pending; this checkpoint is not full DOC compatibility.

## Current checkpoint — Phase 4 PARTIAL, 0.8.0-rc.7, 2026-10-25

Fixed DOCX editor reachability: an oversized centered page could place its first
text outside the viewport. Oversized pages now align to the scrollable origin;
initial focus and viewport changes minimally reveal the focused editor.
Tab from Find opens/focuses Replace directly; Shift+Tab returns to Find.

Native GUI PASS on the rc.7 candidate before the final viewport-size refinement:
open and type without clicking, click/type in a table, Tab to initially hidden
Replace, type replacement, Shift+Tab back. Saved QA text independently checked.
QA file: `manual_phase4_artifacts/input-native-rc7/candidate/replace-copy.docx`.
User source untouched. Final 400%/short-window refinement is automatically tested;
the final exported executable was not separately rerun through native GUI.

Final source gates PASS: fmt, all-targets check, all-targets tests (168 passed,
6 ignored), Clippy with warnings denied, release build. Offline Cargo used.
Deferred manual recovery/printing regression was not run. Phase 4 remains PARTIAL.

Root executable: `C:\NotOffice\LiteOffice-v0.8.0-rc.7.exe`, 4,192,256 bytes.
SHA-256: `3bc5150fef322dbafa6b8f4049b4380609d1739e9e34ac3169df9bcbc6e9c28f`.
Export matches target/release. Earlier accepted/candidate executables preserved.
Branch release/stabilization, HEAD 44a651a; no commit/tag/staging.

## Current work — Phase 4D, text clipping/input fixes, 0.8.0-rc.6 (PARTIAL)

User report, 2026-10-24: the other rc.5 tests succeeded, but the last letter was
visually clipped (screenshot of ЁЖИК in a table) and typing immediately after
Open still did not work. User explicitly clarified **keyboard text input**, not
printing to a device. Remaining rc.5 replacement/Save As/reopen/two-window
conflict scenarios are accepted as user-reported PASS; this does not close the
new rendering/input defects or all of Phase 4. Real printer/PDF requirements
remain separately required; they are not relabeled as keyboard input.

Both defects were reproduced before fixing, using the real full toolbar and
preview through a headless entry point shared with the native eframe update:

- `full_preview_keeps_last_cyrillic_and_wide_glyphs_visible_at_all_view_zooms`:
  FAIL, ЁЖИК ink ended at x104.8 while the field clip ended at x100.3.
  The display used a character-count estimate at a different effective glyph
  width and forgot the true rendered bounds/editor padding.
- `full_ui_accepts_first_typed_text_after_open_without_pointer_or_tab`: FAIL,
  the document stayed Original after a Text event because no editor had focus.

Fixes:

- Render width is measured with exactly the single-line layout job/font used
  by TextEdit, including glyph advance/mesh bounds, side margins and a small
  rounding allowance. The pure approximate pagination estimator is not used to
  size/clip the editor anymore; print/export source data/settings unchanged.
- Successful DOCX Open/restore gets one-shot focus at the start of its first
  existing text fragment. Focus is set before events, not after rendering, and
  before an old Find field can consume the new document's first input. Explicit
  search/toolbar focus, disabled dialogs and an unfocused window are respected.
  No OS window activation command is issued; focus is not seized every frame.
- Document-generation IDs isolate cursor/undo widget state and reset the
  document ScrollArea on successful replacement. Failed Open does not change
  the generation. Two egui contexts/windows retain independent text/Undo.
- Five new full UI tests PASS: 30 word/zoom/paragraph-or-table paint-bound cases,
  typing on the very first or subsequent frame without mouse/Tab, opening from
  old Find focus, one-shot/modal/Find/Replace behavior and independent windows.

Final gates:

- `cargo fmt --check`: PASS.
- `cargo check --all-targets --offline`: PASS.
- `cargo test --all-targets --offline`: PASS — **164 passed**, 6 optional ignored
  (96 library + 63 shell/view + 2 PDF security + 3 importer/replace/security).
- `cargo clippy --all-targets --offline -- -D warnings`: PASS.
- `cargo build --release --offline`: PASS.
- `git diff --check`: PASS, LF/CRLF notices only.

Root candidate: `C:\NotOffice\LiteOffice-v0.8.0-rc.6.exe`, **4,191,232 bytes**,
SHA-256 `e57f507178ae7d643b4f42b0a07292fc1b809246b46511cafda0a09e3846c0bc`.
Export hash equals `target\release\liteoffice.exe`; SHA-256 sidecar added.
Accepted root v0.7.5 and previous rc.5 remain unchanged (hashes rechecked),
no files removed. Branch release/stabilization, HEAD 44a651a, dirty tree;
no commit/tag/staging. Existing unrelated changes and user documents preserved.

Changed this increment: src/main.rs, Cargo.toml/lock, README_RU.md,
CODEX_MASTER_PLAN.md, MANDATORY_FUNCTIONAL_REQUIREMENTS.md, PHASE4_REPLACE.md,
V1_0_UX_AUDIT_CRITERIA.md, this log; new PHASE4_INPUT_RECHECK.md, root EXE/hash.
No dependencies, parsers, save engine or document-script policy changed.

Native rc.6 retest remains **NOT VERIFIED**; only the short checklist in
`PHASE4_INPUT_RECHECK.md` is needed now. No new manual DOCX/PDF was necessary.
Tests create isolated temporary synthetic DOCX. The user-edited
manual_phase4_artifacts/replace-rc5/replace.docx was not regenerated/overwritten:
it is now 857 bytes, SHA-256
`5811d0f3e451930ce3ff45a49a258a9ae1bbe322341dbffbc5f78ce1d35c4067`.

Scope remains bounded: no text insertion into an empty DOCX with no w:t,
DOC/XLS/XLSX remain read-only pending modern export/edit models, PDF printing
still pending, long fragments wider than the field still internally scroll,
pagination still approximate. Deferred full manual printing/recovery regression
was not performed or waived. Phase 4 remains PARTIAL; no next phase started.

## Earlier checkpoint — Phase 4D / increment 1, DOCX Replace and Delete, 0.8.0-rc.5 (PARTIAL)

User scope update, 2026-10-24: all four requirements in
`MANDATORY_FUNCTIONAL_REQUIREMENTS.md` are mandatory, including independent
windows: immediate printing, DOCX/XLSX export with DOC/XLS formatting, printing
opened PDFs with printer/range/scale, and Find/Replace. The user clarified
**edit imported DOC/XLS → export DOCX/XLSX**, not binary DOC/XLS write-back.
Master plan, continuation instructions, roadmap, README and UX criteria now
reference this amendment; old read-only exclusions no longer waive it.

This increment implements the DOCX portion of replacement, not all FR-01–04:

- Ctrl+H/Replace panel, Replace current and Replace all, Enter in replacement
  field, empty replacement, Unicode literal Find semantics, cross-run and table
  text. Replacement inherits the first affected run's format. Run identities,
  paragraph/table structure, images and non-document.xml ZIP parts are retained.
- Each operation is one Undo/Redo transaction. Ctrl+Z/Ctrl+Y/Ctrl+Shift+Z act on
  the document only outside toolbar text inputs. Find/Replace/font focus and
  modal prompts prevent document-wide deletion/undo; an incoming Ctrl+F/Ctrl+H
  focus transfer also prevents a same-frame document command.
- Pure candidate construction and validation before applying; no source write
  until Save. No-op/invalid replacements keep dirty/history unchanged; cached
  results refresh and current replacement resumes after inserted text. Replace
  all refuses more than 10,000 hits instead of silently processing a prefix.
  Replacement <=1 MiB, output text <=32 MiB, single-line valid XML text only.
- New tests first reproduced empty w:t losing fragment IDs after deletion and
  missing xml:space preservation on newly inserted edge whitespace. Parser and
  writer now retain empty/self-closing text nodes, combine a node's text/CDATA
  into one fragment, and add xml:space="preserve" for edge whitespace.
- The user additionally reported Delete not working in DOCX, certainly after
  Ctrl+A and possibly in an ordinary text field. A failing test reproduced the
  document-wide selection defect: Ctrl+A selected runs for formatting, but no
  delete handler existed. Ctrl+A / Select all → Delete/Backspace now clears all
  selected text, not images or table structure, as one Undo transaction. Empty
  results safely Save As/reopen. A real headless egui fragment test also checks
  selected Cyrillic text and the character right of the caret; those pass.
  Native GUI repetition of both reported deletion cases remains NOT VERIFIED.
- Undo history now retains the latest 100 edits instead of truncating the newest
  entry once full; a 105-edit regression verifies the most recent transaction.

The reported same-window Open issue is withdrawn: the user acknowledged that
referaadi-vorm.docx and v03.docx were identical. Their document XML matches;
the two existing replacement-open tests passed. No speculative Open fix made.

Final automated gates on rc.5:

- `cargo fmt --check`: PASS.
- `cargo check --all-targets --offline`: PASS.
- `cargo test --all-targets --offline`: PASS — **159 passed**, 6 optional ignored:
  96 library + 58 desktop/view + 2 PDF security + 3 importer/replace/security.
  This is 20 more passing tests than rc.4. Native PDF action sentinels remain
  green; no new dependencies or document execution capabilities added.
- `cargo clippy --all-targets --offline -- -D warnings`: PASS.
- `cargo build --release --offline`: PASS.
- `git diff --check`: PASS (LF/CRLF notices only).
- Explicit synthetic fixture export:
  `cargo test --test search_formats export_manual_replace_fixtures --offline -- --ignored --exact --nocapture`:
  PASS (1). Only the new replacement fixture exporter ran, not ignored printing
  or the deferred manual recovery regression.

Initial sandboxed file-replacement tests encountered Windows os error 5/1175;
the full suite was rerun with the normal approved Windows file-operation access
and all passed. No tests disabled and no save safety checks weakened.

New root binary: `C:\NotOffice\LiteOffice-v0.8.0-rc.5.exe`, **4,189,696 bytes**.
SHA-256: `3c7c1d38f35fcbb09126e919040b896c781431bb86443a782acb9b426285638a`.
Export hash equals `target\release\liteoffice.exe`. Accepted v0.7.5 root/archive
hashes remain `b3aaa248f15d18681739760af3e76c132434f41e8e2b140856d7dc62976d4fd9`.
rc.4 is retained unchanged as the previous candidate; no files removed this turn.

Synthetic manual files in `manual_phase4_artifacts/replace-rc5/`:

- `replace.docx`, 849 bytes,
  `9b329b3800bfc65efdce6f37ac7ebbc184514f4ba1a7dc585e2b90fd5cd36040`.
- `expected-cats.docx`, 848 bytes,
  `dca1b45dfaa890e636cacc0bdcbe3dba65b9250711b0dd9e199ed74691fba214`.
- `expected-deleted.docx`, 834 bytes,
  `b4f3c6ce3cc26da977af32c8b57afd8273a7d71195dd49efa35ba38dbc3b18b7`.

Manual new-function checklist: `PHASE4_REPLACE.md`. No real printer job, full
manual recovery/printing regression or external Word cross-open was performed.
New native GUI acceptance remains NOT VERIFIED. DOC/XLS/XLSX editing/formatting
and their modern export paths, input-PDF printing and the comprehensive FR-01–04
multi-window gate remain open. PDF still only renders; DOC/XLS/XLSX are still
read-only. Phase 4 remains PARTIAL; no later phase started.

Source changes in this increment: Cargo.toml/lock, src/docx.rs, src/main.rs,
src/lib.rs, tests/search_formats.rs; new src/replace.rs. Documentation changes:
CODEX_MASTER_PLAN.md, CODEX_CONTINUATION_RULES.md, CODEX_ROADMAP_TO_1_0.md,
README_RU.md, V1_0_UX_AUDIT_CRITERIA.md, this log; new
MANDATORY_FUNCTIONAL_REQUIREMENTS.md and PHASE4_REPLACE.md. New test artifacts,
root EXE and hash sidecar are not staged. Previous dirty changes and all user
documents/settings/recovery data are preserved. Git branch release/stabilization,
HEAD 44a651a, dirty working tree; no commit/tag/staging or registry changes.

Next: accept this bounded DOCX increment; implement safe DOC→DOCX / XLS→XLSX
editable/export models with supported formatting, then PDF print/range/scale and
the mandatory shared multi-window acceptance. Keep deferred regression explicit.

## Earlier checkpoint — Phase 4C, Find/PDF/passive content, 0.8.0-rc.4 (PARTIAL)

User scope, 2026-10-24: continue Phase 4, keep the manual recovery regression
deferred, export the new build to the root, remove intermediate builds; then add
PDF opening and verify that embedded scripts/macros are not supported on open.
This is not a later phase or a declaration of Phase 4 completion.

Implemented:

- Unified literal Find in DOC/DOCX/XLS/XLSX, Ctrl+F, Enter/F3 and reverse
  navigation, wrapping results, exact occurrence counters/context, Unicode-safe
  offsets, cross-run DOCX/table matches, all-sheet spreadsheet matches including
  AD500 through a bounded shifted preview. Search results are cached and
  invalidated after edits/undo/redo/open/restore. Scroll is requested only for
  query/navigation, not every frame. Escape accounts for egui losing focus at
  frame start; a headless test reproduced that defect before its fix.
- PDF as read-only raster viewing through Windows.Data.Pdf, Open/CLI/drop/recent/
  independent windows, page navigation, common zoom/scroll, bounded file/pages/
  raster and two-page texture cache. No PDF text extraction/search, editing,
  form interactivity, passwords or printing. A dedicated native worker renders
  only PNG data; no PDF document action is dispatched by the application.
- Active-content audit for all five formats, explicit extension whitelist,
  external/unsafe worksheet relationship rejection, spreadsheet cached values
  rather than formula evaluation, and UI notice that document scripts/macros
  are unsupported. Office macro/script/field fixtures and native PDF JS/Launch
  sentinels pass. Unknown DOCX parts may still be preserved as inert bytes in
  Save As/recovery; no claim of sanitization or safety in other applications.
- The initially cached Windows 0.52 binding aborted while formatting a malformed
  PDF error under current Rust checks. The reader was changed to Windows 0.62.2;
  the same test now returns an ordinary error. A property test also reproduced
  float rounding to a 2001 px raster despite the 2000 px decoder limit; dimensions
  are now clamped and 99,999 fractional extents are tested. No checks suppressed.

Final gates:

- `cargo fmt --check`: PASS.
- `cargo check --all-targets --offline`: PASS.
- `cargo test --all-targets --offline`: PASS — 86 library + 49 shell/view +
  2 PDF security + 2 importer/security tests = **139 passed**, 5 optional ignored.
- `cargo clippy --all-targets --offline -- -D warnings`: PASS.
- `cargo test --release --test pdf_security --offline -- --nocapture`: PASS —
  both native PDF security tests; exporters/read-only mixed-file QA ignored there.
- `cargo build --release --offline`: PASS.
- `git diff --check`: PASS, only LF/CRLF notices.

New synthetic Find fixtures were explicitly exported once via
`export_manual_find_fixtures` to `manual_phase4_artifacts/search-rc3/` (DOC/DOCX/
XLS/XLSX; all four real importers verified). New PDF-reader fixtures were exported
to `manual_phase4_artifacts/pdf-rc4/`; two native page PNGs were visually checked.
The reader also opened the already existing mixed PDF
`manual_phase3_artifacts/automated-1792772468779/portrait-10mm.pdf`, generated two
PNG pages and retained the source byte-for-byte; both images were inspected for
Cyrillic, table/image and numbers. This was read-only QA of the new reader, not
a new print job. **No actual Print-to-PDF or deferred manual recovery regression
was run, and neither is promoted to PASS.**

Final root candidate: `C:\NotOffice\LiteOffice-v0.8.0-rc.4.exe`, **4,180,992 bytes**,
SHA-256 `0f29c1e041429d1f8f1b33bb83d15176d236992c1a4d0ee6aa920b312a6618c0`.
The exported binary matches `target\release\liteoffice.exe` after the final
raster rounding fix. Root now contains only the accepted v0.7.5 and this rc.4;
verified rc.2 rollback remains in releases with a SHA-256 sidecar.

Native Windows user-facing Find/PDF/dialog acceptance remains **NOT VERIFIED**.
Checklist: `PHASE4_FIND_PDF.md`. Audit and limitations:
`SECURITY_ACTIVE_CONTENT.md`. No general parser-exploit immunity is claimed:
native PDF parsing is in a thread, not an OS sandbox; document data is not
disinfected. Large-file responsiveness and the other Phase 4 criteria remain open.

Intermediate binary handling was explicitly requested by the user. Root rc.2
and rc.2 were hash-checked, checked not running, then sent to the Windows Recycle
Bin. Before that, rc.2 was copied and hash-verified at
`C:\NotOffice\releases\LiteOffice-v0.8.0-rc.2-recovery-checkpoint.exe`:
`799d8a512cd870bb44e7736e235e80fb309119f92068523b4924f7667b6ec595`.
The temporary root rc.3 (search-only) was also hash-checked and recycled after
the combined rc.4 export. These three root executables are recoverable from the
Recycle Bin. Accepted v0.7.5 in both root and releases remains unchanged:
`b3aaa248f15d18681739760af3e76c132434f41e8e2b140856d7dc62976d4fd9`.
Documents, settings, recovery data, source ZIP archives and target were not removed.

Changes in this work: Cargo.toml/lock, src/lib.rs, src/main.rs,
src/file_ops.rs, src/xlsx.rs, src/spreadsheet_ui.rs, README_RU.md, this progress
record, V1_0_UX_AUDIT_CRITERIA.md, and a pointer in PHASE4_CRASH_RECOVERY.md.
Added src/search.rs, src/pdf.rs, src/pdf_view.rs, tests/search_formats.rs,
tests/pdf_security.rs, PHASE4_FIND_PDF.md, SECURITY_ACTIVE_CONTENT.md, synthetic
fixtures and the new executable; archived rc.2. Previous dirty changes in
src/doc.rs/docx.rs/print.rs/print_windows.ps1/startup.rs and all unrelated files
were preserved. Branch `release/stabilization`, HEAD `44a651a`; no commit/tag or
registry writes. New Windows 0.62.2 SDK bindings add ten lockfile packages, all
already cached and resolved offline; no external PDF runtime/DLL installation.

## Earlier checkpoint — Phase 4B, crash recovery, 0.8.0-rc.2 (PARTIAL)

Implemented on 2026-10-23. On 2026-10-24 the user reported manual recovery items
**1–5 PASS** on rc.2: crash/restore, later/repeated crash, Save As/cancel, explicit
discard/close choices, and independent-window/conflict handling. Screenshots
also show discovery, the recovered-copy badge, restored text and Save As.
Item **6 (full regression) remains NOT VERIFIED**, explicitly deferred by the
user; do not run it now or promote earlier automated results to a manual PASS.
The complete recovery gate remains PARTIAL until that regression is performed.
The user accepted Phase 4A's six manual save/close/conflict/print items on rc.2.
Phase 3 remains accepted. This does not close all of Phase 4.

DOCX recovery writes validated immutable generations beside the executable in
`LiteOffice-recovery`, normally within two seconds of UI-observed edits. The
source file/fingerprint is unchanged. Each window has an OS-locked session;
startup skips live/claimed sessions and offers restore, confirmed discard or
later. Restore keeps the checkpoint and requires Save As. Successful Save or
explicit discard clears only the session's own payload; failed saves, cancel,
failed opens and a second crash retain it. Lease-only directories are retained
to avoid lock-path deletion races. There is no application-exit Drop cleanup.

Both checkpoint publication and restore validate the model and CRC of every
bounded ZIP part, including unknown parts; incomplete temp files are not offered.
Two valid generations are retained, and damaged newest files fall back with a
warning. Targeted tests first exposed and then verified fixes for (1) unchecked
corruption of unknown OPC parts and (2) rotation retaining a damaged predecessor
instead of the previous good generation. A pending-write status does not claim
new edits are already durable. Full implementation, limits and manual checklist:
`PHASE4_CRASH_RECOVERY.md`.

Final gates on rc.2:

- `cargo fmt --check`: PASS.
- `cargo check --all-targets`: PASS.
- `cargo test --all-targets`: PASS — 75 library + 42 shell tests (117 total),
  2 optional tests ignored in the standard run. Includes an isolated worker
  forcibly terminated after a durable checkpoint, lock release and restore.
- `cargo clippy --all-targets -- -D warnings`: PASS, no suppressed warnings.
- `cargo build --release`: PASS.
- `git diff --check`: PASS (only LF/CRLF normalization notices).
- `cargo test --bin liteoffice windows_pdf_fixture_regression -- --ignored --nocapture`:
  PASS, explicitly run again after the final code changes. Three real PDFs
  verified under `manual_phase3_artifacts/automated-1792772468779/`.

New root executable: `C:\NotOffice\LiteOffice-v0.8.0-rc.2.exe`, **4,127,232 bytes**,
SHA-256 `799d8a512cd870bb44e7736e235e80fb309119f92068523b4924f7667b6ec595`,
matching `target\release\liteoffice.exe`. The existing root rc.2 remains
`eb3f54e00fadf8177f1606f7743bbd91b5895a6b10b4ed58c531dcd1f9913861`.
Both root v0.7.5 and `releases\LiteOffice-v0.7.5-phase3-accepted.exe` remain
`b3aaa248f15d18681739760af3e76c132434f41e8e2b140856d7dc62976d4fd9`.
New synthetic manual fixture: `manual_phase4_artifacts/recovery-test.docx`, copied
without overwriting any user test result from the Phase 3 print fixture.

This iteration edits Cargo.toml/lock version, src/lib.rs, src/docx.rs,
src/main.rs, README_RU.md, this progress record and V1_0_UX_AUDIT_CRITERIA.md;
adds src/recovery.rs, PHASE4_CRASH_RECOVERY.md, the new fixture, new automated
PDF evidence and root RC2 executable. PHASE4_SAVE_PROTECTION.md now points to
the newer recovery checkpoint; its rc.2 acceptance history is retained.
All pre-existing tracked/untracked work is preserved. Branch remains
`release/stabilization`; no commit/tag, dependencies, registry writes or general
cleanup. Initial sandbox-only tests hit os error 5 on atomic rename; the same
tests and all final gates passed outside that sandbox restriction.

Limits: no power-loss/disk-failure guarantee; edits after the last completed
checkpoint may be lost, especially after a visible recovery-write error. DOCX
checkpoint IO is synchronous; maximum-package responsiveness is not measured.
Storage is local and unencrypted. Legacy DOC sidecar edits are outside this
recovery scope. GUI recovery items 1–5 passed by user report; the remaining
manual regression is deferred and still required for full sub-block acceptance.
Only acceptance documentation changed on 2026-10-24; no code/build/test rerun.
Other Phase 4 UX/search/navigation/About/icon,
reliability and performance gates also remain outstanding.

## Accepted sub-block — Phase 4A, save and data protection (rc.2 history)

Version `0.8.0-rc.2` is an intermediate Phase 4 checkpoint, not an accepted release.
The accepted Phase 3 `v0.7.5` binary was copied before code changes to
`C:\NotOffice\releases\LiteOffice-v0.7.5-phase3-accepted.exe`, with a `.sha256`
sidecar; its hash is unchanged:
`b3aaa248f15d18681739760af3e76c132434f41e8e2b140856d7dc62976d4fd9`.
The root `LiteOffice-v0.7.5.exe` is also retained. The new candidate is exported
separately as `C:\NotOffice\LiteOffice-v0.8.0-rc.2.exe`.

Confirmed defects were first reproduced in failing tests:

- After one same-source save, a second save incorrectly raised SourceChanged
  because the opened-state fingerprint was not advanced.
- Save Copy after an external replacement took media/relationships/unknown ZIP
  parts from the external file and combined them with stale edited document XML.

`DocxPackage` now holds a bounded immutable snapshot of the opened OPC package.
Save Copy uses that snapshot even if the source changes or disappears. Regular
Save adopts the successfully written/reloaded destination and refreshes the
baseline. Fingerprinting also limits bytes read if an input grows concurrently.
Safe replacement, verification and separate backups remain in use.

The shell now distinguishes Save/Ctrl+S from Save As/Ctrl+Shift+S, starts Save As
in the current directory and adopts its new path only on success. Ctrl+O opens a
document through the same unsaved-state guard. Native close requests are checked
after this frame's editor changes and cancelled until Save/Don't save/Cancel is
resolved. Save errors retain dirty state and do not resume pending open/close.
Conflicts offer Save a copy, explicit Reload/discard, or Cancel/keep edits.
Failed reloads retain the current document. Background editing/actions are
disabled while a decision is pending.

Automated state/file regressions cover repeated Save, adopted Save As path,
source deletion, unknown OPC part preservation, save failure, cancel/save/discard
closing, pending open, two-window conflict, copy-out and failed/successful reload.
Full tests: **102 passed, 2 optional tests ignored**; the real Microsoft Print to
PDF regression was explicitly run and passed on 0.8.0-rc.2. PDF evidence:
`manual_phase3_artifacts/automated-1792767997357/`.

`cargo fmt --check`, `cargo check --all-targets`,
`cargo clippy --all-targets -- -D warnings`, `cargo build --release` and
`git diff --check` passed. The exported root candidate is **4,083,712 bytes**,
SHA-256 `eb3f54e00fadf8177f1606f7743bbd91b5895a6b10b4ed58c531dcd1f9913861`,
verified against `target\release\liteoffice.exe`. Both the root v0.7.5 and the
separate accepted archive still match their original SHA-256.
On 2026-10-23 the user reported all six items of the last short manual checklist
PASS on this candidate: repeated Save, Save As, dirty close, dirty replacement
open, two-window conflict choices, and printing after Save As. Screenshots show
the stale-window overwrite being blocked and cancellation retaining edits.
`C:\NotOffice\p4.pdf` was independently rendered and inspected on both pages:
A4 portrait, text origin 10.50/9.92 mm, Cyrillic/wrapping, one 150x75 pt image,
four cell-border rectangles, explicit page break and Page 1/Page 2 footers pass.
PDF SHA-256: `a379c51e55171ca4d077a67edcf2892e727c865571fe7d7a58e3ee673fa597c6`.
The first save/close/conflict block is accepted by this manual scope; this is
not acceptance of the whole Phase 4 or a stable 0.8 release. Manual inaccessible-
path write failure was not part of that six-item checklist and remains
NOT VERIFIED manually (automated error-retention regression passes).

Phase 3 remains accepted. Phase 4 remains PARTIAL: automatic crash recovery is
the next data-protection subtask and has not been implemented or claimed here;
the other planned UX, search, navigation, About/icon, reliability and performance
items are also not complete. No dependencies were added, no cleanup/registry
changes were performed, and no commit/tag was created from the pre-existing
dirty working tree.

## Accepted checkpoint — Phase 3 COMPLETE, 2026-10-23

The user reported manual items 1–6 PASS, including preview zoom/scroll/pages,
cancellation, printer error without loss of unsaved text, independent windows,
Ctrl+S and the two-window same-source save conflict. After the error-206 launch
fix, the user completed the requested preview/cancellation/PDF retry and supplied
fresh `C:\NotOffice\10.pdf` and `C:\NotOffice\25.pdf` from the corrected build.
All four final PDF pages were rendered and visually inspected; the text was
compared with `manual_phase3_artifacts/phase3_print_fixture.docx` with no omissions.

- `10.pdf`: two A4 portrait pages; requested 10 mm margins; text origin
  10.50/9.92 mm (left/top), within the 1 mm GDI text-bearing tolerance.
- `25.pdf`: two A4 landscape pages; requested 25 mm margins; text origin
  25.48/24.91 mm (left/top), within the same tolerance.
- Each output has the referenced image before the explicit page break, four
  table-cell border rectangles and correctly ordered Page 1/Page 2 footers.
  Cyrillic and wraps are readable, with no visible clipping or overlap.
- Final PDF SHA-256: `10.pdf`
  `df3e92a464adbdcf84f2de05a84eeb0835ee9d1679cf60431600efb98037074a`;
  `25.pdf` `3823d5e36e70675d26d2b70264a9ba56ed77eed9d0ab45d4129da9005e7bcaac`.
- Accepted root and target release executables match: 4,056,576 bytes, SHA-256
  `b3aaa248f15d18681739760af3e76c132434f41e8e2b140856d7dc62976d4fd9`.
  All six required automated checks passed on this build (89 regular tests;
  2 optional tests ignored in the normal run; real PDF regression run explicitly).

**Phase 3 is complete for the agreed approximate-rendering scope. Phase 4 has
not started.** Earlier PARTIAL/NOT VERIFIED entries below are retained as history
and are superseded by this acceptance checkpoint. Exact Word layout fidelity and
physical-printer fleet certification were not acceptance claims.

## Phase 0 — stabilized baseline

Status: complete on 2026-10-23. Work stopped here as required by
`CODEX_MASTER_PLAN.md`; no later phase or cleanup was started.

### Initial audit

- Project package: `liteoffice` 0.7.1, Rust edition 2021.
- `Cargo.lock` was present (4,351 lines) and retained without modification.
- Git repository root is `C:\NotOffice`; it had no commits and the project
  was untracked. Pre-existing untracked sibling files, releases, documents,
  settings, and Russian-named folders were left untouched.
- Reproduced with `cargo check --all-targets`:
  - E0583: `src/legacy_layout.rs` was missing despite `mod legacy_layout;`.
  - E0599: `PrintBlock::PageBreak` was used but not defined.
  - A future-incompatible `float_literal_f32_fallback` warning at the legacy
    preview border.

### Recovery changes

- Recreated `src/legacy_layout.rs` as a real shared approximate page model for
  recovered DOC paragraphs and manually anchored raster images. It retains
  item order, places invalid/missing anchors in the unplaced gallery section,
  and keeps an oversized image on a page instead of dropping it.
- Added targeted unit tests for ordering, invalid anchors, and overflow.
- Added `PrintBlock::PageBreak`, manifest record `P`, and PowerShell handling
  so the shared DOC page plan explicitly reaches Windows print preview.
- Replaced the ambiguous `1.0` stroke literal with `1.0_f32`.
- Added `/target-static/` to `.gitignore`; no build artifacts are committed.

### Verification

Toolchain:

```text
cargo 1.99.1 (5f94df478 2026-08-27)
rustc 1.99.1 (58cf8f9c3 2026-10-13)
```

All commands were run from `C:\NotOffice\liteoffice` and passed:

```text
cargo fmt --check
cargo check --all-targets
cargo test --all-targets    # 43 passed, 0 failed
cargo build --release
```

Release output was produced at
`target\release\liteoffice.exe` (4,013,568 bytes). It is intentionally kept
out of Git and has not been copied over any existing release.

### Known limitations

- Binary DOC layout remains explicitly approximate; LiteOffice does not claim
  pixel-perfect Word compatibility or reliable raster-to-shape reconstruction.
- The shared plan guarantees explicit boundaries for the recovered DOC flow,
  but Windows GDI text measurement and printer drivers can still paginate
  individual text differently from the on-screen estimate.
- No interactive printer/PDF smoke test was performed in Phase 0. That remains
  a Phase 3 release gate.
# LiteOffice — Final Development Directive to Codex

## 1. Контекст и главная цель

Ты — ведущий инженер-разработчик проекта **LiteOffice**, самостоятельного лёгкого Windows-приложения на Rust для работы с офисными документами.

**Рабочая директория:** `C:\NotOffice\liteoffice`

**Текущая базовая версия:** v0.7.1.

**Целевая версия:** **LiteOffice v1.0.0 Stable Release**.

Твоя задача — последовательно довести существующий проект до надёжного, воспроизводимого и пригодного для распространения релиза v1.0.0.

Это продолжение действующей разработки, а не новый проект.

Перед началом прочитай полностью:

- `CODEX_MASTER_PLAN.md`
- `CODEX_ROADMAP_TO_1_0.md`
- `CODEX_CONTINUATION_RULES.md`
- `RELEASE_PROGRESS.md`

Указанные документы сохраняют силу. Настоящая директива уточняет порядок исполнения, инженерные требования, контроль качества и разрешение противоречий.

Если обнаружено противоречие, приоритет имеют: сохранность пользовательских данных, фактическое состояние исходников, обязательные проверки качества и явно согласованный функциональный объём LiteOffice 1.0.

## 2. Текущее состояние проекта

Согласно последнему отчёту `RELEASE_PROGRESS.md`, Phase 0 завершена.

Подтверждено:

- Восстановлен модуль `legacy_layout.rs`.
- Исправлена обработка `PrintBlock::PageBreak`.
- Восстановлена компиляция проекта.
- Пройдено 43 теста.
- Успешно выполнена Release-сборка.
- Создан исполняемый файл `target\release\liteoffice.exe`.
- Сохранены предыдущие пользовательские файлы и сборки.

**Не повторяй Phase 0 без выявленной технической необходимости.**

Сначала проверь актуальное состояние Git и `RELEASE_PROGRESS.md`, поскольку работа могла продвинуться после подготовки этой директивы.

Продолжай с первой незавершённой фазы. Если Phase 1 ещё не завершена — начни с неё.

## 3. Продуктовая концепция LiteOffice 1.0

LiteOffice должен оставаться:

- Самостоятельным Windows x64-приложением.
- Написанным преимущественно на Rust.
- Распространяемым в виде компактного переносимого EXE.
- Не требующим Microsoft Office, LibreOffice, OnlyOffice, браузерного движка или облачного сервиса.
- Способным работать полностью офлайн.
- Без обязательной регистрации, телеметрии и сетевых запросов.

### Обязательные возможности

**DOCX:** открытие, просмотр, базовое редактирование текста и форматирования, поиск, выделение, Undo/Redo, безопасное сохранение.

**DOC (Word 97–2003):** извлечение и просмотр основного текста, Unicode, ссылки, восстановленные изображения, ограниченная компоновка, копирование текста, печать. Редактирование бинарного DOC не обязательно.

**XLSX/XLS:** просмотр листов, ячеек и доступных сохранённых значений, переключение листов, поиск и экспорт поддерживаемых данных. Полноценное редактирование таблиц и вычисление формул не являются обязательными для v1.0.

**Печать:** системный диалог Windows, Microsoft Print to PDF, ориентация страниц, поля, текст, изображения, пагинация и нумерация.

**Интерфейс:** масштабирование, навигация по страницам, список последних файлов, Drag & Drop, клавиатурные сочетания, строка состояния и понятные ошибки.

Не заявляй полную совместимость с Microsoft Office или pixel-perfect воспроизведение Word, если это не подтверждено тестами.

Не расширяй обязательный функциональный объём 1.0 за счёт экспериментальных функций.

## 4. Принципы архитектуры

Прежде чем изменять архитектуру:

1. Изучи существующие модули и их зависимости.
2. Составь карту ответственности компонентов.
3. Определи общие структуры данных и точки взаимодействия.
4. Найди дублирование логики.
5. Подготовь минимальный план рефакторинга.

Разделяй:

- графический интерфейс;
- модели документов;
- импорт и экспорт форматов;
- пользовательские настройки;
- редактирование и историю изменений;
- компоновку страниц;
- печать;
- файловые операции и безопасность;
- восстановление после ошибок.

Не создавай новые реализации функций, которые уже существуют.

Не переписывай парсеры DOC, DOCX или XLS без обоснованной необходимости.

Не оценивай успешность рефакторинга по количеству удалённых строк или созданных модулей.

Главный критерий — улучшение сопровождаемости без функциональных регрессий.

## 5. Требования к безопасности документов

Сохранность пользовательских данных имеет максимальный приоритет.

Реализуй и проверь:

- атомарное сохранение через временный файл;
- проверку целостности результата;
- защиту от потери исходного документа при ошибке;
- проверку внешних изменений перед записью;
- обработку конфликтов при одновременном редактировании;
- резервные копии там, где они необходимы;
- восстановление несохранённых изменений после аварийного завершения;
- ограничения размера и сложности входных данных;
- безопасную обработку ZIP, XML и бинарных DOC/XLS;
- отсутствие исполнения макросов из документов.

Recovery-файл нельзя автоматически удалять до подтверждённого успешного сохранения или явного отказа пользователя от восстановления.

Lock-файлы не должны быть единственным механизмом защиты от конфликтов.

Для неподдерживаемых элементов документа выбирай безопасное сохранение без потерь или явный отказ от операции, а не тихое разрушение структуры.

## 6. Обратная совместимость

Обязательно сохрани:

- чтение существующих `LiteOffice-settings.txt`;
- совместимость пользовательских `.doc.lo-layout`;
- работу существующих DOC/DOCX/XLS/XLSX;
- поддержку ранее созданных файлов;
- работу печатного манифеста;
- передачу `PrintBlock::PageBreak`;
- существующую функциональность Undo/Redo;
- поведение масштабирования и пользовательских настроек.

Если потребуется новый формат конфигурации, реализуй миграцию.

В рамках чистого рефакторинга не изменяй без необходимости `Cargo.lock`, Release-профиль, форматы пользовательских файлов и контракт печати.

## 7. UX и интеграция с Windows

Проверь наличие и фактическую работоспособность:

1. Drag & Drop для поддерживаемых документов.
2. Открытия файла через аргумент командной строки.
3. Корректной обработки Unicode-путей и пробелов.
4. Поиска `Ctrl+F` в доступном текстовом содержимом.
5. Выделения и копирования текста DOC.
6. `Ctrl+A`, `Ctrl+C`, `Ctrl+V`, `Ctrl+X`, `Ctrl+Z`, `Ctrl+Y`, `Ctrl+P`, `Ctrl+W` в поддерживаемых контекстах.
7. Явного обозначения режима Read-only.
8. Навигации по страницам.
9. Списка последних файлов.
10. Строки состояния.
11. Иконки приложения в Windows.
12. Понятных сообщений об ошибках.
13. Сохранения настроек между запусками.
14. Необязательной регистрации файловых ассоциаций.

Горячие клавиши должны учитывать фокус ввода. Например, `Ctrl+A` в поле поиска не должен выделять весь документ.

Переносимый режим не должен требовать изменений реестра Windows.

## 8. Единый процесс проверки

После существенных изменений выполняй:

```powershell
cargo fmt --check
cargo check --all-targets
cargo test --all-targets
cargo clippy --all-targets
cargo build --release
```

На ранних фазах Clippy используется для диагностики и регистрации технического долга.

До утверждения релизного кандидата должен быть достигнут строгий контроль предупреждений, включая обоснованное устранение предупреждений Clippy.

Дополнительно проверяй:

- повторное открытие сохранённого DOCX;
- отсутствие повреждений ZIP/OOXML;
- открытие повреждённых и нестандартных файлов;
- сохранность пользовательских настроек;
- совместимость `.doc.lo-layout`;
- печать через Microsoft Print to PDF;
- присутствие изображений в печатном результате;
- корректность порядка текста и графики;
- большие документы и потребление памяти;
- работу без установленного Rust.

Количество тестов не является самостоятельным доказательством качества. Нужны проверки конкретных пользовательских сценариев.

Если тест не проводился, указывай **NOT VERIFIED**, а не PASS.

## 9. Версионирование

Синхронизируй:

- `Cargo.toml`;
- `CARGO_PKG_VERSION`;
- заголовок приложения;
- сведения About;
- документацию;
- название выпускаемого EXE.

Ориентиры:

| Этап | Версия |
|---|---|
| Стабилизированная база | 0.7.1 |
| Функциональный кандидат | 0.8.x |
| Публичная бета | 0.9.0-beta.x |
| Кандидат v1.0 | 1.0.0-rc.x |
| Стабильный выпуск | 1.0.0 |

Не увеличивай версию автоматически только при переходе к следующей фазе.

Версия меняется по достижении фактического milestone.

Релизу 1.0.0 должен соответствовать проверенный комплект исходников, исполняемого файла, документации и тестов.

## 10. Git и точки восстановления

До крупного рефакторинга:

- Проверь `git status`.
- Определи корень репозитория.
- Зафиксируй исходное состояние.
- Сохрани SHA-256 и размер контрольного EXE.
- Проверь `.gitignore`.
- Убедись, что пользовательские документы не попадут в коммит.

Не применяй без отдельного разрешения:

- `git add .`;
- `git clean -fdx`;
- массовое удаление неизвестных каталогов;
- принудительный сброс истории;
- перезапись пользовательских документов;
- удаление старых Release-сборок.

Если Git-коммит невозможен, создай проверяемую резервную копию исходников и укажи причину в отчёте.

Не выдумывай Git identity.

## 11. Работа с фазами

Соблюдай ранее согласованную дорожную карту Phase 0–14.

После каждой фазы:

1. Выполни обязательные проверки.
2. Убедись в отсутствии новых регрессий.
3. Обнови `RELEASE_PROGRESS.md`.
4. Создай безопасную контрольную точку.
5. Зафиксируй оставшиеся проблемы.
6. Определи готовность к следующему этапу.

Если обязательный gate не пройден, устраняй проблему в пределах текущей фазы.

**Не переходи к следующей фазе при незакрытом FAIL.**

Команда пользователя «Продолжай автоматически» не отменяет это правило.

Не объявляй фазу завершённой при наличии неподтверждённых обязательных требований.

## 12. Автономность Codex

Тебе разрешено самостоятельно:

- анализировать исходники;
- создавать и изменять код;
- рефакторить модули;
- писать тесты;
- запускать сборки;
- исправлять компиляторные ошибки;
- устранять предупреждения;
- обновлять внутреннюю техническую документацию;
- создавать контрольные коммиты из проверенных исходников.

Не нужно запрашивать разрешение на каждую небольшую правку.

При этом требуется отдельное разрешение пользователя для:

- удаления неизвестных или пользовательских данных;
- изменения внешних систем;
- публикации релиза в интернете;
- массового изменения структуры Git;
- разрушительных операций;
- изменения согласованного продуктового объёма.

## 13. Критерии LiteOffice 1.0.0

Версию 1.0.0 разрешается признать готовой только после подтверждения:

- стабильной компиляции Windows x64;
- завершения обязательных автоматических тестов;
- отсутствия известных критических ошибок;
- безопасного сохранения DOCX;
- работоспособности поддерживаемых форматов;
- корректной обработки ошибок;
- печати документов с изображениями;
- восстановления после аварий;
- совместимости настроек;
- готовности документации;
- воспроизводимой Release-сборки;
- формирования SHA-256;
- проверенного переносимого EXE;
- завершения обязательных ручных приёмочных проверок.

Если требование не выполняется, не объявляй релиз готовым. Укажи конкретный блокер и продолжай его устранение.

## 14. Очистка проекта

Не выполняй фактическую очистку проекта автоматически.

На этапе инвентаризации подготовь:

- список временных файлов;
- размеры каталогов;
- build-артефакты;
- старые резервные копии;
- дублирующиеся сборки;
- файлы, назначение которых не определено.

`target`, `target-static`, старые EXE и резервные каталоги проверяй отдельно.

После выпуска v1.0.0 представь план безопасной очистки.

Удаление допускается **только после отдельного разрешения пользователя**.

## 15. Формат отчёта

После каждой завершённой фазы обновляй `RELEASE_PROGRESS.md` и выводи краткий отчёт со следующими разделами:

- Phase / Status
- Изменённые файлы
- Реализованные функции
- Результаты тестирования
- Git checkpoint
- SHA-256 артефактов
- Обнаруженные регрессии
- Непроверенные сценарии
- Известные ограничения
- Готовность к следующей фазе

В конце указывай:

`NEXT: Phase N — ...`

## 16. Текущая команда

Прочитай обязательные документы и проверь фактическое состояние репозитория.

Phase 0, согласно последнему отчёту, уже завершена.

Начни с первой незавершённой фазы, предположительно **Phase 1**.

Выполни её полностью, соблюдая изложенные выше правила.

Не повторяй уже выполненную работу, если её результат подтверждён.

Не переходи к следующей фазе до успешного завершения текущей.

Работай автономно внутри текущего этапа и предоставь проверяемый технический результат.

---

## Phase 1 — Architecture and core refactor / PASS

**Date:** 2026-10-23
**Branch:** `release/stabilization`
**Baseline preserved:** Phase 0 checkpoint `589662f` (`stabilized-baseline`) remains intact. The user-confirmed pre-Phase-1 control EXE was `4,013,568` bytes with SHA-256 `0a33c5c0d4ddb8f30ffae0a069846b0c501d0ea273771a9b797c99d7b682b5c5`; the required Phase 1 release build normally replaced `target\\release\\liteoffice.exe`, and the newly built artifact is recorded below.

### Changed files

- `src/lib.rs` — exposes the extracted application services.
- `src/main.rs` — now consumes focused services; DOCX paragraph rendering shares an explicit render-options context instead of carrying duplicate per-call state.
- `src/startup.rs` — command-line document path selection, including a Unicode path with spaces and `--` handling.
- `src/recent.rs` — bounded, deduplicated Recent Files persistence.
- `src/file_ops.rs` — supported-format classification and actionable unsupported-format errors.
- `src/preferences.rs` — compatible parsing, validation, load and save of `LiteOffice-settings.txt`.
- `src/print.rs` — shared printing service, retained from the already-committed Phase 1 extraction.
- `src/legacy_sidecar.rs` — versioned `.doc.lo-layout` load/save with source-size, paragraph/image-count and range validation; format remains `LiteOffice-layout-v1`.
- `src/spreadsheet_ui.rs` — read-only spreadsheet view state and renderer extracted from the desktop shell.
- `src/doc.rs` — warning-free, behavior-preserving slice iteration improvements.
- `V1_0_UX_AUDIT_CRITERIA.md` — revised component-responsibility map and remaining boundaries.

### Implemented and retained behavior

- The app shell opens at most one supported document supplied through the command line and reports ambiguous input instead of silently ignoring an extra path.
- Existing preferences and `.doc.lo-layout` sidecars remain readable; invalid sidecars are rejected without using their placements.
- DOCX, legacy DOC, spreadsheet preview, printing and explicit `PrintBlock::PageBreak` flow remain connected to the same application services.
- Recent Files handling, document-type recognition, settings validation and spreadsheet view logic are independently testable.
- The refactor does not change DOC/DOCX/XLS/XLSX parser contracts or the release profile.

### Verification

All Phase 1 required automated gates passed from the final worktree:

```text
cargo fmt --check                         PASS
cargo check --all-targets                 PASS
cargo test --all-targets                  PASS — 56 passed, 0 failed
cargo clippy --all-targets -- -D warnings PASS
cargo build --release                     PASS
```

The test suite covers Unicode/spaced CLI input, `--` CLI form, duplicate/multiple CLI input rejection, existing settings compatibility, layout-sidecar round-trip/rejection, recent-file ordering, supported-file recognition, DOC/DOCX/XLS/XLSX behaviors, printing manifest page breaks and safe DOCX package save.

**Release artifact rebuilt:** `target\\release\\liteoffice.exe`
Size: `4,018,688` bytes
SHA-256: `b16cf0dd78fac3a0e5471551dc5a7471a2b65d7bc98d3571283742cf7566ad4f`

### Git checkpoint

Existing Phase 1 commits were preserved:

```text
a27e432 phase1: isolate startup and recent document flows
44a651a phase1: extract file, preference, and print services
```

The current Git configuration has no `user.name` or `user.email`. Per the continuation directive, no identity was invented and no commit was attempted. The verifiable Phase 1 source archive below captures the final code extraction; this report records its artifact metadata immediately after creation.

```text
C:\NotOffice\releases\LiteOffice-v0.7.1-phase1-checkpoint-20261023.zip
Size: 113,244 bytes
SHA-256: 6bd28c59495389ef2d049fc8ec3e0020db458fd615df6ef896f6c291d2a37aeb
Companion hash: LiteOffice-v0.7.1-phase1-checkpoint-20261023.zip.sha256
```

### Regressions

None observed in automated checks. `git diff --check` reports no whitespace errors.

### NOT VERIFIED (manual acceptance)

- Opening actual files through Windows Explorer command-line association and Drag & Drop in a live Windows session.
- Visual fidelity and page navigation on representative complex DOC/DOCX/XLS/XLSX files.
- System print dialog / Microsoft Print to PDF, including image placement and output pages.
- Running the portable release EXE on a Windows machine without Rust installed.

### Known limitations

- Version remains `0.7.1`; no milestone justified a version change.
- DOC remains read-only and uses approximate legacy layout; OfficeArt image placement is explicitly marked as verified or unverified as applicable.
- XLS/XLSX remain read-only preview/import paths; formula recalculation and workbook editing are out of scope for this phase.
- Session recovery, autosave and external-modification conflict protection are later roadmap work and have not been represented as complete.

### Readiness

Phase 1 is complete. The automated quality gate is green and Phase 0 artifacts, existing commits and user files were retained. Stop at this checkpoint; do not enter Phase 2 in this turn.

`NEXT: Phase 2 — Document models and data integrity`

---

## Phase 2 — Document models and data integrity / PASS

**Date:** 2026-10-23
**Branch:** `release/stabilization`

### Architecture review and identified gaps

- `DocxPackage` already rebuilt a package beside the destination and used the Windows `ReplaceFileW` path, but it had no package/resource validation, no source-change comparison before same-path save, a predictable temporary name, no post-write ZIP verification, and could replace an existing `.docx.bak`.
- The internal DOCX model deliberately represents editable `w:t` text fragments, their run font size, paragraph alignment/page-break information and table-cell layout. It is not a full OOXML object model. Unsupported package parts and relationships must therefore be copied rather than reconstructed.
- DOC, XLS and XLSX remain separate read-only/import pipelines; this phase does not alter their parser or print contracts, including `PrintBlock::PageBreak` and `.doc.lo-layout` compatibility.

### Changed files

- `src/docx.rs`
  - validates package size, part count, uncompressed aggregate and individual-part limits before parsing;
  - rejects unsafe or duplicate package member names;
  - records a source size/modified-time/content fingerprint at open and cancels a same-file save when the source changed externally;
  - creates an exclusive, Unicode-safe temporary peer file, flushes it, reopens and validates the rebuilt ZIP/XML before replacement;
  - retains unknown parts and relationships by copying their content unchanged, preserves existing backups by selecting a distinct backup name, and keeps the original destination untouched on failures.
- `src/main.rs` — save status now distinguishes a new destination from replacement of an existing file and does not claim a backup when none existed.
- `V1_0_UX_AUDIT_CRITERIA.md` — records the Phase 2 model-level external-change protection and DOCX package guarantees; the remaining conflict UI stays explicitly planned.
- `RELEASE_PROGRESS.md` — this report.

### Automated evidence

New targeted DOCX tests verify:

- edit → save → reopen round-trip, including Unicode;
- preservation of media, unknown parts and malformed-but-unused relationship bytes;
- a package without relationship parts;
- corrupt ZIP and invalid XML rejection;
- unsafe part-path rejection and package-file size limit;
- failed write retaining the source document;
- external modification cancelling same-path overwrite without creating a backup;
- an existing backup is retained and a separate backup is selected.

Final phase gates:

```text
cargo fmt --check                         PASS
cargo check --all-targets                 PASS
cargo test --all-targets                  PASS — 64 passed, 0 failed
cargo clippy --all-targets -- -D warnings PASS
cargo build --release                     PASS
git diff --check                          PASS
```

**Release artifact rebuilt:** `target\\release\\liteoffice.exe`
Size: `4,027,904` bytes
SHA-256: `f61d2852fdacb14f133aa2c9d762cd0ea728e7cf8bd08baac7876aa6095f56fd`

### Git checkpoint

The branch remains `release/stabilization`; Phase 0/1 commits and user files are retained. Git still has no configured `user.name` or `user.email`, so no identity was invented and no commit was made. A verifiable Phase 2 source archive and companion SHA-256 file are created as the checkpoint after this report update.

```text
C:\NotOffice\releases\LiteOffice-v0.7.1-phase2-checkpoint-20261023.zip
Size: 117,963 bytes
SHA-256: d43aac8a435d6e2b245a4a8c4f956674f956faaca10bdde23e3328463d3c0a89
Companion hash: LiteOffice-v0.7.1-phase2-checkpoint-20261023.zip.sha256
```

### Regressions

None observed in automated tests. Existing DOC, XLS/XLSX, print-manifest and sidecar regression tests remain green.

### NOT VERIFIED (manual acceptance)

- Windows Explorer/GUI Save As overwrite, backup naming and external-change message with a real DOCX.
- Opening the output in Microsoft Word or another independent OOXML reader.
- Drag & Drop, Microsoft Print to PDF, print image placement and page appearance.
- Live behavior on a machine without the Rust toolchain and large real-world documents near the configured limits.

### Known limitations

- The fingerprint protects an opened DOCX only when saving over that same source path. A separately chosen existing Save As destination is backed up, but has no prior opened-state fingerprint.
- Relationship XML is intentionally not interpreted or repaired; missing or malformed unused relationships are preserved, while unsupported relationship-dependent content is not advertised as editable.
- The editor remains restricted to supported text/run-size changes in `word/document.xml`; it does not claim full OOXML formatting, field, drawing or Word-layout fidelity.
- Recovery/autosave and a GUI choice for external-change conflicts remain later scoped work.

### Readiness

Phase 2 is complete with the requested data-integrity gate green. Stop at this checkpoint; do not enter Phase 3 in this turn.

`NEXT: Phase 3 — Printing and pagination`

---

## Phase 3 — Printing and pagination (PARTIAL: manual Windows/PDF gate pending)

### Audit and implementation

- Confirmed the existing path: `main.rs` creates ordered `PrintBlock` values, `print.rs` serializes the established `S`/`T`/`I`/`P` manifest, and `print_windows.ps1` uses the Windows preview/print dialogs and GDI+ `PrintDocument`.
- Preserved the existing subsystem rather than introducing a second print pipeline. Legacy DOC continues to use its shared approximate `legacy_layout` page plan.
- Made DOCX screen pagination and printed blocks use the selected orientation and margins, eliminating the former hard-coded portrait/A4 preview plan.
- Hardened the manifest boundary: `P` is now rejected when leading, trailing, or adjacent; malformed settings/records are rejected by both Rust-side validation and the PowerShell helper. This preserves the `PrintBlock::PageBreak`/`P` contract without silently losing page transitions.
- Added explicit print outcome markers. A closed print dialog is now reported as user cancellation, distinct from successful printer submission or helper failure.
- The helper still owns and removes only its exclusive temporary manifest; it validates manifest bounds before creating GDI objects and disposes print resources in `finally`.

### Changed files

- `src/print.rs` — shared page geometry helpers, manifest validation/round-trip tests, deterministic page-break validation, temporary-file flush, and printed/cancelled helper outcomes.
- `src/print_windows.ps1` — strict `S`/`T`/`I`/`P` parsing, empty/ambiguous job rejection, and explicit cancellation reporting.
- `src/main.rs` — settings-aware DOCX page ranges for preview and print, printable empty document pages, and accurate print status messages.
- `V1_0_UX_AUDIT_CRITERIA.md` — Phase 3 print-pipeline evidence.
- `RELEASE_PROGRESS.md` — this report.

### Automated evidence

Added deterministic tests for manifest serialization/parsing, order of text/image/page-break blocks, Unicode, page numbering, invalid page boundaries, empty pages, and a large multi-page job. DOCX pagination tests cover complete non-duplicated multilingual block coverage and changed margins/orientation.

```text
PowerShell AST parse of src/print_windows.ps1         PASS
cargo fmt --check                                    PASS
cargo check --all-targets                            PASS
cargo test --all-targets                             PASS — 70 passed, 0 failed
cargo clippy --all-targets -- -D warnings            PASS
cargo build --release                                PASS
git diff --check                                     PASS
```

**Release artifact rebuilt:** `target\\release\\liteoffice.exe`
Size: `4,029,440` bytes
SHA-256: `05f1d9c3cab0bcaa39d519dd9a9a240a0966e7523a70e8279752dbca11f23271`

### Windows/PDF acceptance

**NOT VERIFIED.** The available computer-use runtime could not start (`os error 3`), so no GUI dialog, physical printer, or Microsoft Print to PDF operation was attempted or represented as successful.

Manual acceptance procedure for a Windows host:

1. Open disposable DOCX fixtures with multiple pages, Cyrillic text and images; also open a legacy DOC with recovered images.
2. Test portrait and landscape plus changed margins, then insert/manual-open a document containing explicit page breaks and a long paragraph.
3. Choose **Print**, select **Microsoft Print to PDF**, and write each result to a new disposable Unicode path.
4. Open every generated PDF independently; verify page count/order, Cyrillic, text, images, margins, orientation, page numbering, and no duplicated or empty unexpected pages.
5. Run once more and cancel the dialog; verify the in-app status says printing was cancelled and no PDF is written. Test an unavailable printer separately and confirm the error returns without a hung window.

### Regressions and limitations

No automated regressions observed; Phase 2 DOCX safe-save tests, DOC/XLS/XLSX, sidecar, recent-files, undo/redo, and existing print-manifest tests remain green.

- GDI+ pagination is intentionally an approximate renderer; actual font metrics and binary DOC layout are not claimed to be pixel-perfect Word output.
- Real printer availability, Microsoft Print to PDF output, and image rendering in a generated PDF require the pending manual gate.
- Recovery/autosave and conflict UI remain outside this phase.

### Checkpoint and readiness

The branch remains `release/stabilization`; existing commits, archives, and user files are preserved. Git has no configured author identity, so no identity is invented and no commit is created. The source-only checkpoint excludes user files and build artifacts:

```text
C:\NotOffice\releases\LiteOffice-v0.7.1-phase3-checkpoint-20261023.zip
Size: 122,596 bytes
SHA-256: ecf4ce18a6f1ce5b6a2560e14248c4a367fdd524e67adc9bc6a8a20cbcabf776
Companion hash: LiteOffice-v0.7.1-phase3-checkpoint-20261023.zip.sha256
```

**Phase 3 is not complete:** all automated gates pass, but the required Windows/PDF manual acceptance gate is `NOT VERIFIED`. Do not start Phase 4 from this checkpoint.

### Post-checkpoint hotfix — document switching diagnostics

- Reworked the open path as a transactional operation: the current document, selection state and editing history remain untouched until the new package loads successfully.
- Added a retained, visible **Open diagnostics** trace: selected file, unsaved-edit decision, type check, package load, model replacement and UI completion. Opening errors name the failed stage and are rendered in red in the status area.
- Added regression tests for DOCX `A → B`, failed loading of B retaining A, and unsaved edits holding the pending B path without changing A.

```text
cargo fmt --check                         PASS
cargo check --all-targets                 PASS
cargo test --all-targets                  PASS — 73 passed, 0 failed
cargo clippy --all-targets -- -D warnings PASS
cargo build --release                     PASS
git diff --check                          PASS
```

**Current release artifact:** `target\\release\\liteoffice.exe`
Size: `4,032,000` bytes
SHA-256: `c70784f73b285604d85aff8493e26c5e977c3539441311312cc323038449ff31`

### Additional Phase 3 Windows/PDF evidence and DOCX-switch crash fix

**Real Microsoft Print to PDF submission: PASS.** User acceptance evidence confirms two independently generated PDFs: both open, have six A4 pages, retain Cyrillic and extractable text, contain six embedded raster images, and are visually identical across the two prints.

The Windows extended Print dialog may report that its own built-in preview is unavailable. This is not a print failure: the existing PowerShell/GDI pipeline already opens a separate .NET `PrintPreviewDialog` before the system printer dialog. Wiring the shell dialog's proprietary preview command would require a different XPS/COM-oriented print architecture and is not justified here. The application-level preview itself remains a manual UX check.

Observed DOC pagination is consistent with the documented approximate binary-DOC model:

- Page 2 can contain only a short paragraph because `legacy_layout` makes a conservative whole-paragraph height estimate and never silently drops content.
- The six recovered images appeared on pages 4–6 because no verified DOC shape-to-raster anchors were available. Unanchored images are intentionally emitted after text under `Recovered images (unplaced, not original Word layout)` rather than being falsely placed beside unrelated text.
- Empty space on image pages follows aspect-ratio preservation and the one-image-per-page fit rule. It is not evidence of missing print blocks. Users can assign verified/manual image anchors or save a `.doc.lo-layout` sidecar to improve this approximate layout.

**Critical DOCX A → B crash fixed.** During a single egui frame, the toolbar could load B after preview ranges had been calculated for A. Rendering then indexed B with A's stale ranges. The preview flow and ranges are now produced together only after toolbar actions, from the active model. The change preserves unsaved-edit protection and does not reset a document before the replacement package is known-good.

New regression coverage includes a large A / short B stale-plan fixture; all indices of the replacement plan must be within B's flow. Combined automated result: `74 passed, 0 failed`.

```text
cargo fmt --check                         PASS
cargo check --all-targets                 PASS
cargo test --all-targets                  PASS — 74 passed, 0 failed
cargo clippy --all-targets -- -D warnings PASS
cargo build --release                     PASS
git diff --check                          PASS
```

**Current release artifact:** `target\\release\\liteoffice.exe`
Size: `4,032,000` bytes
SHA-256: `734d7f8e97796ac1a0dee8bd093fd529b85c82dc1801271d27f7a98b4f744045`

Phase 3 remains **PARTIAL**: cancellation, unavailable-printer behavior, orientation/margin output, explicit preview UX, and comparison with the source document still require manual evidence. Do not start Phase 4.

### v0.7.5 — independent Windows document windows

The package version, lockfile root package and window title now use **0.7.5** via `CARGO_PKG_VERSION`.

- `Ctrl+N` and **New window** start a blank, independent LiteOffice process.
- **Open in new window…** selects one supported file and starts a new process with the file path as one OS-native argument. It does not use a shell or concatenate a command line.
- **Open in this window…** remains explicit replacement behavior and retains its unsaved-DOCX confirmation flow. `Shift` + Drag & Drop opens the dropped file in a new independent window.
- Each process owns its document model, search, scroll, zoom, undo/redo, print task and unsaved state. File Explorer associations already start a normal independent process; startup itself never spawns child windows, preventing launch loops.
- The native title is updated to `[* ]<file name> — LiteOffice v0.7.5`; `*` marks unsaved DOCX edits. Existing DOCX source-fingerprint protection remains the cross-window save-conflict check.

Automated command-construction tests cover blank windows, Unicode/space paths and ten independent document commands. They do not launch arbitrary child processes during tests.

```text
cargo fmt --check                         PASS
cargo check --all-targets                 PASS
cargo test --all-targets                  PASS — 77 passed, 0 failed
cargo clippy --all-targets -- -D warnings PASS
cargo build --release                     PASS
git diff --check                          PASS
```

**v0.7.5 release artifact:** `target\\release\\liteoffice.exe`
Size: `4,035,584` bytes
SHA-256: `1334b39ff2d56ca4aadec2ec9723073ea1aa46126c8d5f537f95029cf91c8257`

**NOT VERIFIED manually:** simultaneous launch of ten native windows, Alt+Tab/taskbar entries, `Ctrl+N`, Shift+Drop, Explorer association, independent closing, and a two-window same-DOCX save conflict. These require an interactive Windows desktop; source-fingerprint conflict regression tests remain green.

`NEXT: Phase 3 — Windows/PDF manual acceptance gate`

### 2026-10-23 — Phase 3 preview images and executable PDF regressions

Phase 3 is still **PARTIAL**. Phase 4 has not started.

- Confirmed and repaired a DOCX screen/print mismatch: images were appended to
  the first print page but absent from the screen flow. Drawing relationship
  references now determine the supported PNG/JPEG instances and their paragraph
  positions. Unreferenced package media is omitted, repeated references remain
  repeated, and external/unsafe relationship paths are not loaded.
- Screen preview and the print manifest now consume the same image blocks,
  physical dimensions, footer reservation and page-break decisions. Source
  drawing extents are used in Original mode. Image fitting is still approximate;
  floating Word shapes and table-cell image positioning are not full Word layout.
- Added bounded texture decoding for DOCX and preserved raster indexes on failed
  DOC decoding. Undecodable images have a visible placeholder instead of silently
  shifting the following images to another anchor.
- The user's `10.pdf` and `25.pdf` were printed from a legacy **DOC**, not the
  synthetic DOCX fixture. A read-only headless check of that local DOC decoded and
  painted all six recovered images in its existing gallery after the text. This
  confirms rendering commands, not the native window's visible scroll position.
- Replaced the old source-string renderer assertion with behavioral coverage.
  Regular tests cover relationship order, unused media, shared page/size decisions
  at 50–400% zoom, an actual egui image mesh, and manifest bounds/roundtrips.
- Explicit Windows regression command:
  `cargo test --bin liteoffice windows_pdf_fixture_regression -- --ignored --nocapture`.
  It creates a synthetic DOCX, runs the same GDI helper with Microsoft Print to PDF
  without dialogs, and calls `tests/verify_phase3_pdf.py` (requires Python/PyMuPDF).
  The test asserts that printing did not modify the source DOCX.
- Actual evidence in `manual_phase3_artifacts/automated-1792746316684/`:
  `print-regression.docx`, `portrait-10mm.pdf`, `landscape-25mm.pdf`, and
  `portrait-no-numbers.pdf`, plus manifests and page PNGs. All three PDFs have two
  pages, readable Cyrillic, two table-cell border paths and the referenced raster
  before the source page break. The image size matches the preview (144×72 pt).
  Measured text origins: 10.50/9.92 mm and 25.48/24.91 mm (left/top); allowed
  tolerance is 1 mm for GDI text bearings/printer unit rounding. Numbering is
  present when enabled and absent when disabled. All six PDF page renders were
  visually inspected.
- A second opt-in regression,
  `local_legacy_doc_preview_image_regression`, takes a local DOC from
  `LITEOFFICE_DOC_PREVIEW_TEST_INPUT`; it never embeds the user's document in the
  test suite or changes it. It passed with six decoded and painted images.

Manual evidence reported before this patch: cancellation, unavailable-printer
handling, portrait/landscape, 25 mm PDF output, Cyrillic/text/images/page numbering,
Ctrl+S and independent windows passed. Those results remain historical evidence.
After this rendering change, repeat the six original manual acceptance items on
the new release, especially native preview image visibility/zoom/scroll and DOCX
table borders using `manual_phase3_artifacts/phase3_print_fixture.docx`.
The two-window same-source DOCX save-conflict GUI case remains **NOT VERIFIED**:
the earlier reports did not establish two distinct writes to the same source
path, and the on-disk evidence contained only one replacement backup.
No manual item is promoted to PASS by a headless test. Do not close Phase 3 until
the full manual gate is repeated and supported by evidence.

Validation for this patch: `cargo fmt --check`, `cargo check --all-targets`,
`cargo clippy --all-targets -- -D warnings` passed. `cargo test --all-targets`
passed with **87 tests, 2 opt-in tests ignored**; both opt-in tests were then
explicitly run and passed as described above. The sandboxed all-target run
initially denied Windows ReplaceFile/rename operations in temporary directories;
the identical test command passed outside the sandbox. These permission errors
were not changed into product workarounds. Release build and final diff check are
recorded in the accompanying recheck status artifact.

### Phase 3 follow-up — Windows launch error 206

The user's screenshot confirmed that the rebuilt GUI could not start print
preview: `Cannot open Windows print preview ... os error 206`. The embedded
PowerShell helper had grown beyond the Windows command-line limit once encoded
as UTF-16/base64. The preceding PDF regression used `-Command` and therefore
failed to cover the GUI's `-EncodedCommand` launcher. Its passing PDFs did not
establish that the GUI could launch printing.

The production launcher now passes a short, constant encoded bootstrap. The
trusted helper and manifest are written to an exclusively owned temporary job
directory, read as UTF-8, and removed after either success or failure. User
content and file paths remain environment data. Windows execution policy is not
changed. Both normal dialogs and the PDF regression use the same Rust launcher.

New regressions execute a helper far larger than the Windows command-line limit,
check the actual command remains under 2048 UTF-16 units, verify Unicode manifest
data survives, and check temporary cleanup on successful and failed execution.
The real DOCX/PDF regression passed through this shared launcher and produced
evidence in `manual_phase3_artifacts/automated-1792766514600/`.

`cargo fmt --check`, `cargo check --all-targets`, `cargo test --all-targets`
(89 passed, 2 optional tests ignored), and
`cargo clippy --all-targets -- -D warnings` passed. The PDF optional test was
explicitly run and passed. Phase 3 remains PARTIAL pending manual confirmation of
native preview launch, cancellation and fresh PDF output from this corrected
executable. The user's earlier 1–6 PASS report is retained, but the subsequently
observed print-launch failure prevents closure.
