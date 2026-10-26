# LiteOffice v1.0.0 — criteria from the external UX audit

This is an additive acceptance document. `CODEX_MASTER_PLAN.md` remains the
authoritative staged plan and its completed phases are not reordered. The
referenced `CODEX_ROADMAP_TO_1_0.md` was not present in the working tree when
this document was created; this file records the audit requirements without
attempting to recreate or replace that missing roadmap.

## Acceptance rules

- The 2026-10-24 user amendment in `MANDATORY_FUNCTIONAL_REQUIREMENTS.md`
  adds mandatory FR-01–04: immediate printing, DOCX/XLSX export and DOC/XLS
  formatting, input-PDF printing with printer/range/scale, and Find/Replace
  current/all with Ctrl+F/Ctrl+H. All require independent-window acceptance.
  Earlier read-only exclusions are current limitations, not a waiver.

- A button, label, or code path alone is not acceptance evidence. Each item
  needs an automated test where practical and a Windows behavior check where
  it interacts with the GUI, clipboard, shell, or printer.
- Existing settings and `.doc.lo-layout` sidecars remain compatible.
- DOC remains explicitly approximate. It is currently read-only, but FR-02
  requires imported-text/format editing with DOCX export; no criterion permits
  a claim of pixel-perfect Word compatibility or native binary DOC write-back.
- Recovery files are additional data. Never remove a usable recovery copy
  until the source has been saved successfully or the user explicitly rejects
  recovery.

## UX and reliability matrix

| ID | Requirement | Actual status at Phase 1 start | Planned acceptance phase and evidence |
|---|---|---|---|
| UX-01 | Drop DOC/DOCX/XLS/XLSX onto the window | Present through egui dropped-file input; not yet covered by an automated behavior test. | Phase 4: verify all four extensions, Unicode paths, dirty-document prompt, and useful failure status. |
| UX-02 | Open one document from command-line arguments, including Unicode and spaces | Implemented in Phase 1 with `args_os`; one path or `-- path` is accepted, multiple paths report an error. | Phase 1: unit tests for Unicode/spaces, separator, and multiple arguments; Windows smoke test remains Phase 5. |
| UX-03 | Unified Ctrl+F search with navigation for all textual views | DOCX-only toolbar search exists; DOC/XLS/XLSX are missing. | Phase 4: one search state/model, Ctrl+F focus behavior, next/previous navigation, and tests per format. |
| UX-04 | Copy selected DOC text correctly | Missing. | Phase 4: real clipboard behavior and selection tests/manual check with Cyrillic text. |
| UX-05 | Explicit read-only mode | Spreadsheet preview says read-only; DOC lacks a consistent mode indicator. | Phase 4: persistent mode badge and disabled unsupported actions for DOC/XLS/XLSX. |
| UX-06 | Detect external edits and prevent invisible overwrite | Phase 2 records a size/timestamp/content fingerprint and blocks a same-path DOCX overwrite when it differs; an explicit reload/save-copy/cancel UI is still missing. | Phase 4: present the explicit choice and cover the GUI conflict path. |
| UX-07 | DOCX crash recovery without changing original before confirmed save | Missing. | Phase 4: atomically write and validate recovery package, offer recovery at startup, retain it until save succeeds or explicit rejection; test malformed and interrupted recovery. |
| UX-08 | Standard shortcuts with correct text-input focus handling | Ctrl+P and a DOCX-specific Ctrl+A exist; focus handling is incomplete. | Phase 4: Ctrl+O/S/Shift+Ctrl+S/F/P/Z/Y/A and copy behavior, with text-field focus tests. |
| UX-09 | Convenient page navigation | Pages are shown in a scroll view only. | Phase 4: page selector/next/previous controls tied to the shared page model, with bounds tests. |
| UX-10 | Clear file-operation errors | Individual open/save messages exist but are inconsistent. | Phase 4: normalized operation/path/reason statuses, including unsupported, missing, malformed, permission, and conflict errors. |
| UX-11 | Preserve and improve Recent Files | Portable eight-entry list exists and is now isolated/tested for ordering; write failures and unavailable entries are not surfaced. | Phase 4: safe persistence, prune/missing-file UX, Unicode paths, and tests. |
| UX-12 | Useful status bar | A bottom status line exists. | Phase 4: include document mode, filename, dirty/conflict/recovery state, and operation outcome; test state text. |
| UX-13 | Windows application icon | Missing. | Phase 4: embed an ICO in the Windows executable and verify Explorer/window presentation without adding a runtime dependency. |
| UX-14 | Optional file associations without harming portable EXE use | Missing. | Phase 4: explicit opt-in registration/removal with no automatic registry writes; manual Windows validation. |

## Cross-cutting automated tests required before v1.0.0

- CLI launch argument parsing, Unicode paths, path-with-spaces handling, and
  rejected ambiguous invocations.
- Opening each supported format by CLI and drag/drop, including malformed and
  unsupported files with actionable errors.
- DOCX safe save, external-change conflict, failed save, recovery write,
  recovery validation, restart discovery, explicit discard, and post-save
  recovery cleanup.
- Recent-files ordering/persistence and compatibility of settings and
  `.doc.lo-layout` data.
- Search, read-only actions, page navigation, and keyboard shortcuts with
  focus in editable and non-editable controls.

## Phase 1 architecture record

- `src/doc.rs` reads legacy DOC text, links, OfficeArt anchors, and recovered
  rasters; `src/legacy_layout.rs` is the shared approximate DOC page plan for
  preview and printing. `src/legacy_sidecar.rs` owns the compatible
  `LiteOffice-layout-v1` sidecar format.
- `src/docx.rs` owns OOXML parsing, model mutation, package rebuilding, and
  atomic replacement/backup. It limits package/part resources, rejects unsafe
  or duplicate package paths, validates the temporary rebuilt package before
  replacement, preserves untouched parts byte-for-byte at the content level,
  and detects a changed source before same-path overwrite. `main.rs` renders
  its `DocumentModel` and now derives print blocks from the same DOCX
  flow/page plan used by preview.
- `src/xls.rs` and `src/xlsx.rs` load read-only `SpreadsheetModel` sheets;
  `src/spreadsheet_ui.rs` owns spreadsheet view state and rendering, while CSV
  export remains in the desktop shell.
- `src/print.rs` and `src/print_windows.ps1` serialize ordered text/image/page
  blocks into the Windows print preview path.
- Startup argument parsing is isolated in `src/startup.rs`; Recent Files
  ordering/storage is isolated in `src/recent.rs`; supported-file
  classification is in `src/file_ops.rs`; and settings serialization is in
  `src/preferences.rs`. The remaining desktop shell deliberately retains the
  cross-format toolbar and one-window app coordination; session/recovery state
  is scheduled for the explicit Phase 7 model-stabilization gate.

## Phase 3 print-pipeline record

Acceptance: **COMPLETE on 2026-10-23**, for the documented approximate layout
scope. User-reported manual items 1–6 passed, including the two-window save
conflict; after the Windows launch-error fix the requested manual retry was
completed. Fresh root `10.pdf` and `25.pdf` were inspected on all four pages and
verified against the DOCX fixture: portrait/10 mm, landscape/25 mm, image, table
borders, Cyrillic, explicit page break and page numbers passed. Exact output
hashes, measured origins and the accepted EXE hash are in `RELEASE_PROGRESS.md`
and `manual_phase3_artifacts/RECHECK_STATUS.md`. Phase 4 has not started.

- `main.rs` creates one ordered `PrintBlock` stream. Legacy DOC uses the same
  approximate `legacy_layout::paginate` plan as its preview; DOCX preview and
  printing use the same orientation/margin-aware approximate page ranges.
- `print.rs` serializes the stable `S` (settings), `T` (UTF-8 text), `I`
  (legacy image) and `P` (page-boundary) records, plus `R` (table row) and `D`
  (sized DOCX image). It rejects ambiguous leading, trailing or adjacent `P`
  records and writes an exclusive temporary manifest/helper directory. The GUI
  and actual-PDF regression share the short Windows process launcher.
- `print_windows.ps1` validates that contract, displays Windows preview and
  printer selection, reports a distinct printed/cancelled result marker, and
  renders with GDI+ page settings. It remains an approximate rendering path;
  manual Windows evidence and actual-PDF regression results are recorded above.

## Phase 4A/B checkpoints — 0.8.0-rc.2 and 0.8.0-rc.2

- UX-06: an external-change conflict now presents reload/discard, save-copy and
  cancel/keep choices. Tests cover two independently opened sessions and check
  that a copied package retains its originally opened OPC parts. The user
  reported the two-window conflict choices PASS on 2026-10-23; the supplied
  screenshot also shows stale-window overwrite blocked while edits remain.
- Save now refreshes the fingerprint/baseline; Save As changes the session path.
  Native close and replacement-open requests use Save/Don't save/Cancel guards;
  failed save/reload retains dirty text and does not approve closing. These
  state transitions have automated tests; the user reported dirty close/open,
  repeated Save and Save As PASS on 2026-10-23. Print-after-Save-As PDF was also
  inspected successfully. Manual inaccessible-path write failure remains
  NOT VERIFIED; see the scope in `PHASE4_SAVE_PROTECTION.md`.
- UX-08: Ctrl+S now performs normal Save; Ctrl+Shift+S performs Save As; Ctrl+O
  uses the guarded open path. The remaining shortcut/focus work is not complete.
- UX-07: rc.2 adds durable per-window recovery checkpoints, full ZIP CRC/model
  validation, startup restore/discard/later, live-session exclusion via OS locks,
  fallback to a predecessor, and Save As for recovered documents. Tests include
  a forcibly terminated child process, interrupted/damaged generations, failed
  writes, cleanup and mixed DOCX print-plan preservation. On 2026-10-24 the user
  reported native recovery checklist items 1–5 PASS. Full regression (item 6)
  is NOT VERIFIED and explicitly deferred; the full recovery gate remains
  PARTIAL. See `PHASE4_CRASH_RECOVERY.md`. This is not a
  guarantee against power loss or edits newer than the last completed checkpoint.
- The accepted v0.7.5 executable is preserved separately with a verified hash.
  Phase 4 as a whole remains PARTIAL; no later phase has been started.

## Phase 4C — rc.4, user-approved PDF scope extension, 2026-10-24

- UX-03: common literal Find for DOC/DOCX/XLS/XLSX, Ctrl+F focus, Enter/F3 and
  reverse navigation, bounded result count, Unicode-safe ranges, cross-run DOCX
  and all-sheet/far-cell spreadsheet navigation. Headless/import tests pass;
  native GUI acceptance remains NOT VERIFIED. PDF is explicitly raster-only and
  outside text Find (no PDF text extraction/OCR implemented).
- PDF open/CLI/drop/recent/new-window are added at the user's explicit request;
  native Windows read-only bitmap rendering, bounded files/pages/raster/cache,
  page selector and shared zoom. No interactive PDF actions or document scripts.
- User's active-content requirement is recorded in `SECURITY_ACTIVE_CONTENT.md`:
  no VBA/XLM/DDE/JS execution for all five allowed formats, cached spreadsheet
  values only, external worksheet target rejection, synthetic execution-sentinel
  and readonly tests. This is not malware removal or a parser-vulnerability guarantee.
- The deferred recovery/manual print regression remains NOT VERIFIED; it was
  not run or waived by these new reader/search tests. Phase 4 is still PARTIAL.

## Phase 4D — rc.5, mandatory scope amendment and DOCX replacement, 2026-10-24

- FR-01–04 are mandatory as recorded in MANDATORY_FUNCTIONAL_REQUIREMENTS.md,
  including all-window behavior. User chose DOC/XLS editing → DOCX/XLSX export;
  native binary save-back is not required. Modern export/format models and PDF
  printer/range/scale are pending, not waived by the old read-only matrix.
- DOCX Replace current/all, Ctrl+H, atomic model transaction, cross-run/table
  Unicode ranges, empty replacement, Undo/Redo and toolbar focus guards added.
  Package preservation, save/reopen and two-window conflict tests pass.
- Confirmed Ctrl+A→Delete defect fixed for selected DOCX text; tables/images
  remain. Headless selected-text/caret Delete, global Delete/Undo and Save As
  tests pass. Native user Delete/Replace checks remain NOT VERIFIED.
- Empty DOCX text-node identities, edge whitespace serialization and the latest
  100 Undo entries now have regression tests. Total automated suite: 159 PASS,
  6 optional ignored. This is not manual acceptance or a closed Phase 4.

## Phase 4D — rc.6, input readiness and text clipping

- User reports the remaining rc.5 checks successful, except the last glyph
  clipping and typing immediately after Open; clarified keyboard input, not a
  printer request. Earlier new-function GUI uncertainty is superseded only by
  that bounded user report, not by a claim of complete Phase 4 acceptance.
- Both defects reproduced in full toolbar/preview tests before fixes. Display
  widths now use actual font advances/ink and editor margins; first DOCX input
  receives one-shot focus before event processing, with per-document IDs and
  modal/toolbar guards. Read-only formats are unchanged.
- Five new headless UI tests cover first-frame typing, replacement of a document
  while old Find had focus, one-shot/modal behavior, independent windows and
  30 short-word/zoom/paragraph-or-table paint-bound checks. Total 164 PASS;
  native rc.6 retest NOT VERIFIED. See PHASE4_INPUT_RECHECK.md. No deferred
  manual printing/recovery regression was performed or waived.
