# LiteOffice — continuation roadmap to 1.0

**Current roadmap, 2026-10-25:** read `ROADMAP_TO_1_0_RU.md` first. It records
the accepted rc.21 short manual checks, current Phase 4 gaps and the ordered
path to beta/RC/1.0. The detailed historical phase lists below remain a checklist;
do not repeat completed implementation or treat their stale "pending" entries
as current status. Current mandatory scope remains FR-01–04 plus basic TXT/MD/RTF.

**This document EXTENDS, and does not replace, `CODEX_MASTER_PLAN.md`.** Execute its Phase 0–6 first. The work in Phase 7–14 below begins only after the earlier stabilization gates are met. Treat v0.8.0 RC described there as a *stabilization checkpoint*, not permission to skip the remainder. No stage is complete merely because `cargo build --release` succeeds.

## Mission / non-negotiable constraints

**User scope amendment, 2026-10-24:** `MANDATORY_FUNCTIONAL_REQUIREMENTS.md`
overrides the earlier exclusions below for DOC/XLS formatting, DOCX/XLSX
export, printing input PDFs and Find/Replace. These are required, including
independent-window behavior. Current read-only limitations remain truthful
implementation status, not the target definition of done. The user clarified:
edit imported DOC/XLS and export to DOCX/XLSX; native binary write-back is not required.
Implement the amendment incrementally within the current Phase 4; do not skip
later phase gates or execute the user-deferred manual regression without consent.

Create a portable, offline Windows x64 application in Rust, with no required Word, Excel, LibreOffice, Chromium, Python, Java, external server, or internet connection at runtime. Preserve all verified capabilities already in the project. Keep source DOC and XLS unchanged unless an explicit supported save path exists. Respect ownership/backup semantics and never overwrite an input without opt-in and verified round-trip. Use the existing egui/eframe stack unless a measured problem justifies migration. Deliver a **bounded, truthful 1.0**, not a full replacement for Microsoft Office.

**Definition of 1.0 scope (amended 2026-10-24):** a robust lightweight viewer/editor with supported DOC/DOCX and XLS/XLSX formatting, export to DOCX/XLSX, immediate Office/PDF printing and Find/Replace, all in independent windows. Native binary DOC/XLS write-back, full Word-fidelity pagination, comprehensive PDF editing, macro support, charts editing and Microsoft Office parity are not required. Editing imported DOC/XLS must preserve original files and export modern formats. Absent features stay explicitly pending until implemented and verified; see FR-01–04.

## Target format & feature matrix (validate against implementation; don't assume)

| Format | Read | Render | Edit | Save | Export/print | 1.0 acceptance |
|---|---|---|---|---|---|---|
| DOCX | Yes | paragraphs/tables/basic formatting/images where verified | text runs/paragraphs, font size, basic selectable formatting; only safely supported runs | Safe Save As DOCX with package-preservation guarantees bounded and documented | TXT, Windows Print/PDF | Round-trip preserves untouched package parts; edits persist and document opens in Word-compatible readers |
| DOC (97–2003) | Recover text/links/images where possible | approximate text/images; supported formatting and existing sidecar | Required imported-text/format editing; currently pending | Export DOCX; no native binary DOC save | DOCX, TXT, print/PDF from approximate layout | supported content/format round-trip, original unchanged, honest approximation banner |
| XLSX | Actual cells/types and supported formatting | sheet navigation and readable grid | Required basic cell/format editing and replacement; currently pending | Export XLSX with supported formatting; pending | XLSX, CSV/TXT, print | coordinates/types/values/styles retained within explicit support matrix, no silent CSV substitution |
| XLS | Read supported BIFF cell data and formatting | simple grid with supported formatting | Required imported-cell/format editing; currently pending | Export XLSX; no native binary XLS save | XLSX, CSV/TXT, print | bounded parser, original unchanged, exported model/format checks |
| PDF | Required, read-only raster reader implemented in rc.4 | Native Windows pages/zoom | No PDF editing required | No PDF save/edit required | Required print to selected printer with page range and scale; pending | distinguish opening/printing input PDF from generating Office PDF; no script/action dispatch |

Do not expand this into full Office/PDF parity or faithful binary DOC layout.
The explicitly approved basic editing/export/printing requirements above are
not optional post-1.0 work. Preserve existing viewers while implementing them.

## Release cadence / phase sequence

Every phase: preserve a rollback checkpoint (commit/tag), update `RELEASE_PROGRESS.md`, add regression tests BEFORE marking done, run `cargo fmt --check`, `cargo check --all-targets`, `cargo test`; run Clippy and Release at milestone boundaries. Present changed files, test outcomes, known risks, next phase. Keep stages sequential. Avoid large speculative rewrites.

### Phase 7 — v0.8.x: viewer/editor interaction and core model stabilization
- Establish a single document/session state model: format, source file, dirty state, capabilities (read/edit/save/print/export), errors, undo stack and sidecar ownership.
- Clearly distinguish selection of a run, all text, and text-input focus. Ctrl+A respects text-field focus. Font family/size UI reflects selection; missing/mixed styles are not silently normalized.
- Zoom 50–400%, +/- controls, numeric input, Fit width and optional Fit page; remembered per user, no modification of underlying DOCX formatting or print scale. Avoid horizontal toolbar clipping at narrow widths.
- DOCX editing safety: rich text change only for supported runs; don't destroy mixed formatting, field codes, tables or unknown XML. Disable edit with explicit reason when unsupported. Batch format/Undo uses a single transaction. Save As only with sanity-check and backup/temp file replacement protocol.
- Distinguish modified DOCX content from DOC image-sidecar changes. Warn about unsaved changes on close/open/exit; discard or save only by explicit user action.
- Accessibility minimum: keyboard navigation, legible system fonts, Unicode fallback, focus indicators and tooltips.
- **Exit gate:** complete supported DOCX edit/save/reopen/undo scenarios on the three known fixture classes (simple, mixed runs, tables); zoom/focus/dirty-state GUI smoke tests; no loss of untouched DOCX parts in byte/package inventory.

### Phase 8 — v0.8.x: unified pages, print and image placement correctness
- Separate `DocumentFlow -> PagePlan -> Display/Print` into pure/testable engine where possible. One page-break decision for viewer and printer; record tolerated font-metric differences (egui vs GDI+), don't promise pixel-perfect Word.
- A4 Portrait/Landscape, 5–40mm margins, page numbers, images Original/Fit width/Width %, avoid image clipping/splitting, preserve aspect ratio, provide page navigation and actual page-count estimate from PagePlan.
- Add exact print-image manifest contract with golden tests. Ensure original path, sidecar, and print assets survive multi-page jobs. Rendering should never depend on inaccessible transient relative paths.
- Verify DOC manual image positions after paragraphs, save/restore `.lo-layout`, verified OfficeArt anchors where available; no automatic image pairing without evidence. If OfficeArt mapping remains partial, preserve manual mode and show coverage/counts.
- **Exit gate:** 1-page and multi-page DOC text+image tests, Portrait/Landscape, Cyrillic, oversized images, page number not overlapping content. Check Microsoft Print to PDF visual outputs. Regression on large user document performed locally without committing it.

### Phase 9 — v0.8.x: parsing, saving and file safety hardening
- Fuzz-oriented/property tests for corrupt OPC ZIP, duplicate ZIP paths, path traversal, gigantic deflate ratio, XML entity/recursion, invalid UTF-8, malformed CFB/FIB/CLX/OfficeArt, BIFF length errors and image decompression bombs. Enforce per-input and aggregate resource caps with meaningful error messages.
- Atomic writes: temp in same volume + fsync where suitable + replace; backup original and never truncate existing DOCX on error. Recover gracefully from canceled save, permission errors, disk full, interrupted process.
- Macro execution disabled. External relationships/hyperlinks not automatically opened; explicit user click only and allow only supported URL schemes. No telemetry or network requests by default.
- Test extreme cases: empty file, Unicode filenames, long paths, read-only files, multiple concurrent documents (or explicitly one-document limitation), 3–10 MB DOC and spreadsheet, malformed files.
- **Exit gate:** no panic in negative tests, bounded processing, documented file support limitations, test/fuzz summary, data-loss regression suite green.

### Phase 10 — v0.9.0 Beta: packaging and practical usability
- New/open/recent; meaningful File > Save and Save As **only when supported**; PDF via Print to PDF clearly labeled; print preview, page settings, feedback for unsupported formats. Recoverable errors in UI, not just terminal.
- About includes version from `CARGO_PKG_VERSION`, 1.0 license notices, supported matrix and diagnostic mode/version, without private path exposure in logs by default.
- Portable preferences: use adjacent `LiteOffice-settings.txt` when writable; otherwise fallback to `%APPDATA%\LiteOffice\` or another documented per-user Windows app-data path; migration preserves existing settings. Do not scatter files silently.
- Include icons, proper filename and version metadata, optional code signing (not prerequisite if certificate unavailable). Stable start without terminal window. Define minimum OS and dependencies empirically (Windows 10/11 x64 test preferred).
- Validate release without Cargo installed and offline on a clean Windows VM or independent machine. Measure cold startup, typical document load, peak RAM and binary size; record baselines instead of fabricating targets.
- **Exit gate:** `0.9.0-beta.1` reproducible portable EXE with an end-user Russian guide, limitations matrix, optional English UI documentation, basic screenshot/smoke evidence.

### Phase 11 — v0.9.x: golden-file / cross-application compatibility suite
- Build **redistributable synthetic fixtures only**: DOCX styles/tables/list/fields/Unicode/headers/images; DOC legacy hyperlinks/embedded+floating photos/shape anchors; XLS and XLSX dates, sharedStrings, formulas with cached values, varied cell types; image-heavy and multi-page print fixtures. User-supplied documents stay outside Git and release artifacts.
- Keep checksums of input files; prove opening/printing doesn't mutate inputs; prove round-trip edits are restricted and unchanged OPC parts remain intact within explicit constraints.
- If Word/Office, WPS, or another real compatible viewer is available **and allowed**, manually open produced DOCX and compare; absence must be reported, not faked. PDF print golden tests can compare text/positions/number of pages and visual snapshots with documented tolerances.
- Build a machine-readable support matrix plus manual acceptance sheet with columns (fixture, operation, expected, actual, screenshot or log, pass/fail/skip + reason).
- Regression gates on every candidate; bug fixes first, feature expansion stops at 0.9.0 beta feature freeze.
- **Exit gate:** all P0/P1 scenarios pass, no known reproducible data loss; high-risk unknown cases explicitly disabled, not papered over.

### Phase 12 — 1.0 release candidate: feature freeze and quality gates
- Freeze scope. Fix P0 and P1 defects; P2 only if safe and targeted. No new document engines or dependency migrations without explicit approval.
- Execute ALL: `cargo fmt --check`; `cargo check --all-targets`; `cargo test`; `cargo clippy --all-targets -- -D warnings`; `cargo build --release`; optional `cargo audit` if tooling/internet is available. Don't declare scan passes when offline.
- Verify 0 warnings (or enumerate justified exceptions rather than blanket `allow`). Create stable release branch/tag, record toolchain, lockfile state, compiler, OS, build flags and exact artifact SHA-256.
- Run Windows manual smoke matrix: DOCX editing + save/reopen, DOC + verified/manual images + sidecar, XLS/XLSX, Cyrillic fonts, preference persistence, zoom, portrait/landscape print with pictures, paginated PDF, no data mutation, unsupported-format UX.
- **Exit gate:** produce signed-off `RC_ACCEPTANCE.md` listing observed outcomes; no unclosed P0/P1 bugs, no unexplained failed tests, rollback possible.

### Phase 13 — v1.0.0 release and reproducible distribution
- Release only after Phase 12 gates; version set in `Cargo.toml`, window, EXE metadata (where feasible) and docs consistently via one source. **Do not call build 1.0 unless acceptance passed.**
- Deliver: `LiteOffice-v1.0.0-win-x64.exe`, `SHA256SUMS.txt`, `README_RU.md`, `SUPPORTED_FORMATS.md`, `KNOWN_LIMITATIONS.md`, `CHANGELOG.md`, `THIRD_PARTY_LICENSES.md`, `BUILDING.md`, `RELEASE_REPORT.md`. Optionally ZIP portable bundle if runtime asset files/scripts are required — verify standalone-EXE claim by actual test.
- Confirm licensing obligations for dependencies, test malware false-positive hygiene, icon/EXE version data if applicable, and absence of embedded private documents/tokens.
- Archive clean source commit/tag `v1.0.0`, lockfile, release binary and acceptance evidence. No auto-publishing to GitHub or third-party sites without the user's explicit approval.

### Phase 14 — post-release cleanup (AFTER v1.0.0, never before)
- Inventory candidates with sizes and dry-run: `target`, `target-static`, scratch downloads, extracted ZIPs, stale EXEs, logs and temporary fixture output. Classify by category: definitely reconstructable, versioned sources, user data, unknown.
- Keep at least: clean Git repo, source+Cargo.lock, unit/synthetic fixture tests, source DOC/DOCX/XLS/XLSX/PDF documents, settings and `.lo-layout` files, release 1.0 binary+hash, previous known-good build, backup snapshot, release notes.
- Do **not** call `cargo clean`, delete `target`, prune old backups, move user files, or erase untracked data until a human reviews the dry-run list and explicitly approves. Preserve user-created settings and all original test documents.
- After approval, execute only specified deletions, record log and bytes freed; recheck EXE opens after cleanup.

## Hard blockers / escalation to user

Ask for an explicit decision before: breaking change to DOCX format, removal of an existing capability, deleting any user file or unknown file, replacing a dependency stack, broad change to target format scope, publishing release, applying a paid/license-restricted dependency, or setting version 1.0 while P0/P1 tests fail.

When external integration/tools/VM needed and not connected, do the best available local test and mark check `NOT VERIFIED`; never claim manual Windows smoke or cross-app compatibility that wasn't actually performed. Do not run autonomously overnight unless the user explicitly enables a suitable scheduler/agent workflow.

## Codex continuation protocol (minimum output after EACH phase)

`Phase N status: PASS / BLOCKED / PARTIAL`

1. Changed files and rationale (paths).
2. Tests run with exact command and pass/fail counts.
3. Behavior verified / not verified; errors and limitations.
4. Git checkpoint identifier, clean/dirty state; rollback instructions.
5. Remaining blockers and next phase.
6. Updated `RELEASE_PROGRESS.md` with checklist and cumulative change log.

Codex may perform one phase without requesting approval for routine file edits; stop on hard blockers and for the cleanup approval gate. Never suppress a failing phase and start the next one as if it had passed.

## Success condition

The application is **v1.0.0** only after feature-scoped acceptance, safe DOCX editing round-trip, robust viewers, consistent printing, portable Windows build, documentation and verified regression gates. Feature parity with Word/Excel/PDF editors is expressly outside scope. Prefer a smaller stable 1.0 over an ambitious but broken 1.0.
