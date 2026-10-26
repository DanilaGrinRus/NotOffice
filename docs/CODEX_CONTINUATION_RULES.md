# Codex continuation rules — after Phase 0

This file *clarifies* the earlier plans and does not replace either one.

## Mandatory user scope update — 2026-10-24

Read `MANDATORY_FUNCTIONAL_REQUIREMENTS.md` before planning or implementation.
FR-01 immediate printing, FR-02 DOCX/XLSX export and DOC/XLS formatting,
FR-03 input-PDF printing (printer/page range/scale), and FR-04 Find/Replace
(current/all, Ctrl+F/Ctrl+H) are required across independent windows. Earlier
roadmap exclusions do not waive these user-approved requirements. Track partial
support honestly; do not mark a cross-format feature complete for DOCX alone.
DOC/XLS clarification: edit imported documents and export to DOCX/XLSX; native
binary DOC/XLS write-back is not required. Preserve the original input files.
Keep Phase 4 active and the user-deferred manual regression NOT VERIFIED.

## Verified baseline (report from user)

This is the historical Phase 0 baseline. For the current Phase 4 checkpoint,
candidate and pending manual gates, read the top of `RELEASE_PROGRESS.md`.
Do not restart completed phases or apply historical patches over current code.

- `RELEASE_PROGRESS.md`: Phase 0 complete 2026-10-23.
- 43 tests passed; format, all-target check, all-target tests and Release passed.
- `src/legacy_layout.rs` and `PrintBlock::PageBreak` repaired with actual implementation and tests.
- Source package v0.7.1; compiled EXE 4,013,568 bytes; older released EXEs unchanged.
- Git root `C:\NotOffice`, with no commits at original audit and many untracked sibling/user files.
- Printing/PDF manual smoke was NOT done in Phase 0.

## Ordering

1. Phase 1–5: architecture, features, print and initial release gates.
2. Phase 6: **ONLY inventory and dry run**; do not perform deletions or `cargo clean`.
3. Phase 7–13: scoped v0.8 through v1.0 release work; do not skip if earlier candidate exists.
4. Phase 14: after successful release + **explicit separate approval**, execute only approved cleanup.

## Tests each phase

`cargo fmt --check`, `cargo check --all-targets`, `cargo test --all-targets`; additional tests from relevant phase and `cargo build --release` at each milestone. For P0/P1 severe issues stop; never silently advance. Record exactly what ran and results.

## Version rules

Keep Cargo, displayed version, docs and exported EXE in sync via `CARGO_PKG_VERSION`. Do not mark 1.0 released until phase 13 gate is met; do not interpret `0.8.0-rc` as a final release.

## Manual gate

Request end-user Windows interaction where needed: real printer, Microsoft Print to PDF, clean-machine portability, GUI focus, Microsoft Word cross-open. If no interaction possible mark `NOT VERIFIED`, not PASS. Don't block on irrelevant cosmetic issues if known and documented, but don't waive data-loss / severe security defects.

## Git safety

Do not run `git add .` or `git clean -fdx` from `C:\NotOffice`. Review and stage explicit paths, respect project `.gitignore`, avoid uploading/committing external documents, personal data, unknown binaries, files adjacent to source. Git branch/checkpoint only if safe. State accurate checkpoint/hash, or why unavailable.
