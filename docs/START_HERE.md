# Start Codex here

In a trusted Developer PowerShell/Command Prompt:
```powershell
cd C:\NotOffice\liteoffice
 git status --short
 cargo --version
 rustc --version
```

Open Codex **in that same project directory**, and paste the message below. You can copy `CODEX_MASTER_PLAN.md` into `C:\NotOffice\liteoffice\CODEX_MASTER_PLAN.md` first.

> Read CODEX_MASTER_PLAN.md fully and follow it as a staged release-engineering plan. Start with Phase 0 **only**. Examine the actual working tree, recover the missing `legacy_layout.rs` and fix missing `PrintBlock::PageBreak` properly, run fmt/check/test/release, commit a compilable checkpoint, and report changed files, exact commands and results. Stop after Phase 0 for review; do not clean files or proceed to feature expansion automatically.

When Phase 0 is confirmed, advance with:

> Continue CODEX_MASTER_PLAN.md with Phase 1 only. Make small verified changes; run required checks, record progress and stop for review.

Repeat for phases 2 through 6. Never run cleanup without explicit approval.
