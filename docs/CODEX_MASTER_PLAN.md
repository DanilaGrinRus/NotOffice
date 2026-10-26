# LiteOffice — Codex release engineering plan

Project root: `C:\NotOffice\liteoffice` (Windows, Rust MSVC). Current checkpoint:
Phase 4 PARTIAL, 0.8.0-rc.22; read `RELEASE_PROGRESS.md` for verified gates and
`MANDATORY_FUNCTIONAL_REQUIREMENTS.md` for approved scope. The current ordered
roadmap is `ROADMAP_TO_1_0_RU.md` (2026-10-25). Historical starting baseline was
broken 0.7.1 after tested 0.7.0; the Phase 0 repair below is history,
not an instruction to restart completed phases or apply old patches. Do **not**
declare a release or phase complete from compilation alone.

## Ground rules
- User-approved scope update 2026-10-24: `MANDATORY_FUNCTIONAL_REQUIREMENTS.md`
  is required reading. Immediate print availability, DOCX/XLSX export plus
  DOC/XLS formatting, printing opened PDFs with printer/range/scale, and
  Find/Replace current/all with Ctrl+F/Ctrl+H are mandatory in independent
  windows. Earlier read-only exclusions describe old scope, not a waiver.
- Operate only inside project root unless explicitly exporting release artifacts to `C:\NotOffice\releases`.
- Examine actual files and `git status` first. Don't assume latest ZIP patch files were applied correctly.
- Do not remove user `.doc`, `.docx`, `.xls`, `.xlsx`, `.pdf` files, settings, `.lo-layout` files, or untracked files. No `git clean -fdx`, no recursive deletion of the project.
- Preserve standalone Windows GUI operation and current DOC/DOCX/XLS/XLSX functionality. Do not introduce LibreOffice, Word, Chromium runtime, cloud services, or paid dependencies.
- Work in small commits/checkpoints on branch `release/stabilization`, if git is available. Never commit binaries, secrets, sample personal documents, or `target`.
- Before every code change: reproduce errors, identify root cause, write/adjust targeted tests. After each phase: `cargo fmt --check`, `cargo check --all-targets`, `cargo test`; record results in `RELEASE_PROGRESS.md`.
- Don't claim format-perfect Word compatibility. Distinguish verified behavior and approximate rendering.
- Do not change Rust edition to 2026; use the supported value already in Cargo.toml (2021 or 2024).
- If blocked, preserve compilable state and report blocker; do not mask errors or disable tests.

## Phase 0 — audit and recover compiling tree (mandatory FIRST)
1. Inventory file tree, Cargo.toml, Cargo.lock, git changes, modules and renderer/print paths.
2. Reproduce current compile errors. Known issues: `mod legacy_layout;` in `main.rs` without `src/legacy_layout.rs` (E0583); `print::PrintBlock::PageBreak` referenced in `main.rs` but missing from `print.rs` (E0599). Warning: `egui::Stroke::new(1.0, ...)` should use `1.0_f32` when required.
3. Restore `legacy_layout.rs` from a **verified compatible** checkpoint/patch or recreate with appropriate tests; do not add empty stubs. Align shared-layout page model with renderer AND print pipeline. Add a real `PageBreak` variant and handle it in all matches + PowerShell print manifest/rendering, or remove reference only if an equivalent consistent mechanism exists. Don't lose pagination.
4. Run `cargo fmt --check`, `cargo check --all-targets`, `cargo test`, and `cargo build --release` successfully. Make clean checkpoint and label it `stabilized-baseline`.

## Phase 1 — architecture & correctness
1. Map features and data flow by format: DOC text/OfficeArt recovery and `.lo-layout` sidecar; DOCX XML text + format/save; XLS/XLSX read-only; Windows print path; preferences.
2. Extract cohesive modules from oversized `main.rs`: file operations, preferences, toolbar, zoom, document preview, print integration, app state.
3. Eliminate duplicate layout logic, avoid split-brain page break calculations, maintain backward compatibility of settings and `.lo-layout`.
4. Add tests for each module and verified sample fixtures; no unvalidated structural rewrites.

## Phase 2 — document/rendering improvements (scoped MVP)
1. Viewer zoom 50–400%, buttons `-`/`+`, numeric entry and Fit width; preferences persist. Zoom affects **view only**, not DOCX underlying font size or printing.
2. Make page preview legible and consistent with shared page model. Mark approximation for binary DOC; don't claim pixel-perfect Word.
3. DOCX: verify font size edits, Select All, Undo/Redo, safe Save As, reopen round-trip. Preserve original ZIP package parts, unknown relationships where possible; back up before replacing.
4. DOC: test anchors (floating vs inline), recovered images, field handling, manual placement and `.lo-layout` restore; don't silently guess image-to-shape matches.
5. XLS/XLSX: verify open and existing export features, stable spreadsheet cell read.

## Phase 3 — printing & pagination
1. Compare screen preview and Microsoft Print to PDF using same page block plan where possible; confirm pages/ordering of text+images, no clipping, orientation, margins 5–40mm, page numbers, print-image modes Original / Fit width / Width %.
2. Handle oversized images, page breaks, Unicode/Russian text, no missing images, no unexpected duplicate/unplaced images.
3. Build regression fixtures (small floating DOC, mixed inline DOC, Cyrillic DOC, DOCX with tables and unicode, XLSX with strings/numbers). Don't use large user documents in repository commits.
4. Document actual limitations (not exact legacy DOC layout and not full Office suite).

## Phase 4 — production polish
1. Correct app/package version from one source (`env!("CARGO_PKG_VERSION")`), About dialog, README, file description if feasible. Use a release candidate semantic version (`0.8.0-rc.2` suggested; don't assert 1.0 prematurely).
2. UX: File/Open/Save/Save As/Print buttons enabled only where supported, meaningful error messages, dirty/unsaved indicators, keyboard shortcuts, clipping/overflow under narrow windows.
3. File security & reliability: ZIP bomb protection, bounds checks, malformed CFB/OLE inputs, decoding caps, handle IO failures, atomic saves or backup, no macros execution. Add regression tests.
4. Performance: sample 3.5MB binary DOC, memory/time to open, UI responsiveness, EXE size. Avoid speculative rewrites and new dependencies without measurements.

## Phase 5 — automated release gates
Required commands:
```powershell
cd C:\NotOffice\liteoffice
cargo fmt --check
cargo check --all-targets
cargo test
cargo clippy --all-targets -- -D warnings
cargo build --release
```
- If clippy strictness finds pre-existing warnings, fix them or explicitly report bounded exceptions (do not globally suppress warnings).
- Manual acceptance on Windows 10/11: open DOC/DOCX/XLS/XLSX, DOCX change/save/reopen, DOC images+anchors+sidecar, zoom, restart preferences, print to PDF portrait/landscape including images and page numbers.
- Publish `RELEASE_REPORT.md` with tests, size, SHA-256, screenshots (optional), known bugs, supported/unsupported features and repeatable build commands.
- Release artifact: `C:\NotOffice\releases\LiteOffice-vX.Y.Z-win-x64.exe`; do not overwrite stable build.

## Phase 6 — SAFE cleanup ONLY AFTER a passing release gate
1. Show dry-run listing of candidates: `target\` (rebuildable Cargo artifacts), `target-static\` (verify purpose), previous exported EXEs, old build ZIPs, redundant backups and logs.
2. Do not delete anything without a confirmed safe inventory and user approval. Explicitly preserve `.git`, `src`, `tests`, `Cargo.toml`, `Cargo.lock`, `README*`, documents, settings, layouts, and last 2 known-good releases.
3. `cargo clean` only after release EXE has been built, copied, hashed and smoke-tested. Warn Cargo will rebuild dependencies on next compilation.
4. Record bytes freed and list of preserved files.

## Definition of done
A release candidate only when compilation, automated tests, smoke tests and PDF regression passes; no blockers; reproducible release EXE and rollback; readable Russian README and explicit known limitations. Otherwise stop at a checkpoint and report outstanding tasks.
