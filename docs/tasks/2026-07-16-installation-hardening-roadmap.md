# Installation Hardening Roadmap

Date: 2026-07-16  
Branch: `feature/installation-orchestrator`  
Baseline: `edde7196` (`ci: add bounded macOS verification mode`)  
Status: active

## Purpose

This is the execution ledger for completing the guided installer safely. It records the decisions approved during the `grill-me` interview, the required implementation order, and the evidence needed before moving to the next phase. Chat history is not the source of truth for progress.

Every phase must be implemented as a bounded task. A Worker self-review is necessary but not sufficient: the coordinator must inspect its files and validation evidence before marking the phase complete. A failed build or test stops that phase at the first failure; fix it before starting later work.

## Locked product decisions

### Existing installations and install location

- Preserve existing runnable Agent CLIs in place. Never silently migrate, remove, or overwrite them.
- If an existing installation is not writable, offer an explicit **switch to managed installation** action. Only after confirmation may the managed path take precedence; the old installation remains untouched and is reported as shadowed.
- New npm-backed installs use managed per-user roots:
  - Windows: `%LOCALAPPDATA%\Agent-Manager\npm`
  - macOS: `~/.agent-manager/npm`
- Managed npm cache is separate from the user's existing npm cache and is capped at 500 MiB.
- Prefer a verified official installer when a tool has one. An npm fallback must be disclosed; it cannot silently replace the official strategy.
- npm-backed tools install the latest upstream version from a fixed package-name allowlist.
- Install each version into an isolated directory. Verify the command before atomically switching the managed entry point. Retain only the current and previous verified versions.

### Environment, PATH, and permissions

- Production preflight must measure real available space and actual path access; it must not use `u64::MAX` or fixed writable values.
- Require at least 1 GiB free on the temporary volume and managed-install volume. If both paths share a volume, count it once. Never clean user files automatically.
- Detect path states explicitly: writable, needs creation, requires elevation, blocked, or not applicable.
- Never follow unexpected symlinks, Windows reparse points, junctions, owner anomalies, or paths escaping the managed root.
- After explicit confirmation, prepend the managed executable directory to the user PATH so the selected managed version wins. Preserve and report shadowed installations.
- Windows updates the user PATH without touching unrelated entries and refreshes the process environment. A failed write restores the original value.
- macOS automatically supports zsh and bash only. Use one idempotent, clearly marked PATH block in the appropriate login profile.
- Do not retain full macOS profile backups because profiles may contain secrets. Use same-directory atomic replacement and immediate in-operation rollback. Never log profile contents.
- A malformed, duplicated, or partial managed block is a hard stop with a clear manual-action message; do not guess or rewrite it.
- Other macOS shells receive a precise manual PATH command instead of automatic file modification.
- If installation succeeds but PATH persistence fails, keep the installed files and report `installed_not_runnable` with retry and manual-repair actions. Do not attempt an unsafe npm rollback.
- Uninstalling Agent Manager preserves managed CLIs and PATH by default. A future explicit cleanup action owns their removal.

### Task lifecycle and UI

- The confirmation page lists every change but uses one **Confirm and continue** action. Mandatory changes are not represented as redundant checkboxes; only genuinely optional actions use checkboxes.
- A stale or mismatched confirmation discards the old task and performs one fresh preflight. A second mismatch stops with a localized state-changed message; raw internal errors are not shown.
- Only one install task may run globally. Other install buttons remain disabled; no implicit queue is created.
- Network-only transient failures retry at most twice with bounded backoff. Integrity, signature, permission, and declined-authorization failures never auto-retry. An authorization prompt appears at most once per user action.
- Download and confirmation stages are cancellable. Do not kill a running MSI/PKG installer. Between tools, cancellation stops remaining work without rolling back completed tools.
- A batch prepares shared dependencies once, installs tools sequentially, continues after an isolated tool failure, and reports each result independently.

### Diagnostics, CI, and release

- Diagnostics are opt-in. Redact usernames, home paths, environment values, tokens, proxy credentials, and shell-profile contents before preview.
- **Send to developer** opens a prefilled public GitHub Issue only after the user reviews the report. Never embed a GitHub token in the application and never silently upload logs.
- Upgrade deprecated GitHub Actions and pin third-party Actions to reviewed full commit SHAs. Dependabot may propose updates but must not auto-merge them.
- Default macOS verification runs on Apple Silicon. Release candidates additionally run the full automated suite on a GitHub Intel macOS runner.
- A stable release requires the bounded Windows Sandbox acceptance scenario and at least one real Apple Silicon beta install. Intel remains a documented compatibility risk until real hardware evidence exists.
- Initial beta: 2–3 Windows users covering missing Node and existing old Node, plus 1–2 Apple Silicon users. Any blocking install failure pauses expansion.

## Execution phases

| Phase | Deliverable | Status | Acceptance evidence |
| --- | --- | --- | --- |
| 0 | Record decisions and execution ledger | Complete | Roadmap commit `211b45db`; Worker aligned to it |
| 1 | Cross-platform system-access probes | Complete | Accepted commit `b4f7fbd2`; Windows/macOS artifact-free CI passed |
| 2 | Structured path-access snapshot | Complete | Accepted commit `a1a9fa72`; Windows/macOS artifact-free CI passed |
| 3 | Managed latest-version npm installs | Complete | Accepted through `478888df`; Windows/macOS artifact-free CI passed |
| 4 | Safe user PATH persistence | Complete | Accepted through `22e907f6`; Windows/macOS artifact-free CI passed |
| 5 | Lifecycle and confirmation UX | Complete | Accepted through `8b061682`; Windows/macOS artifact-free CI passed |
| 6 | Opt-in diagnostic reporting | Not started | Redaction fixtures, local preview, prefilled GitHub Issue, no embedded credential or silent upload |
| 7 | CI hardening and dual-platform gates | Not started | Pinned Actions; bounded Windows and Apple Silicon success; Intel release-candidate workflow |
| 8 | Controlled acceptance and packaging | Not started | Small Windows Sandbox pass, beta evidence, one retained Windows package set, one macOS DMG, ZIPs preserved |

## Phase details

### Phase 1 — Cross-platform system-access probes

Owner: Paseo Worker `bed6b7e2-5e36-4c66-9545-bed410d7c733`  
Workspace: isolated Paseo worktree  
Allowed behavior: real disk space and temporary-directory write/delete probes on Windows and macOS only.

Acceptance checklist:

- [x] Windows keeps `GetDiskFreeSpaceExW` behavior behind a focused interface.
- [x] macOS uses `statvfs` with checked arithmetic and structured failures.
- [x] Write probes use collision-resistant create-new semantics and clean up after success.
- [x] File paths and unwritable directories are rejected without residue.
- [x] Linux remains unsupported and platform code is correctly gated.
- [x] Worker reports self-review, changed files, commands, and risks.
- [x] Coordinator inspects every change and reruns proportionate validation.
- [x] Windows and macOS GitHub verification both pass with artifact building disabled.

### Phase 2 — Structured path access

Owner: Paseo Worker `0c2cfb53-6abe-4467-85ef-b8642a083ba7`
Workspace: isolated Paseo worktree
Status: Complete; the Worker exhausted its provider quota after producing an uncommitted draft, and the coordinator reviewed, corrected, validated, and integrated the bounded change.

- [x] Replace ambiguous production writability booleans with explicit access states without changing test-fixture convenience constructors unnecessarily.
- [x] Resolve and validate the temporary, managed install, and managed cache paths. Missing directories are evaluated through a safe existing ancestor without creating user data during read-only preflight.
- [x] Measure each distinct backing volume and apply the 1 GiB threshold.
- [x] Treat unavailable npm paths as not applicable until Node/npm repair completes; never misclassify absence as permission denial.
- [x] Reject files or metadata-inaccessible paths during disk-path resolution instead of silently walking to a parent directory.
- [x] Coordinator directly inspected all five changed files and verified scope against the phase contract.
- [x] Local formatting, metadata, and diff checks pass; local Rust execution remains unavailable because MSVC `link.exe` is not installed.
- [x] Windows and macOS GitHub verification both pass with artifact building disabled.

### Phase 3 — Managed npm installation

Owner: Paseo Worker `b5637e36-6cc6-4a26-9975-a8d2beab02bf`
Workspace: isolated Paseo worktree `phase3-managed-npm`
Status: Complete; the coordinator directly reviewed the Worker commit, fixed one compiler lifetime error and two cross-platform fixture paths, and accepted the final artifact-free platform evidence.

- [x] Add a fixed package allowlist and resolve `latest` at the start of the confirmed task.
- [x] Stage into a tool/version-specific directory beneath the managed root. Pass prefix/cache paths as process arguments, never interpolated shell text.
- [x] Reject unsafe path components and filesystem indirection before every write boundary.
- [x] Verify the installed executable and version in a clean environment, then atomically update the managed entry point.
- [x] On failure, keep the previous entry point. Remove only coordinator-owned stale versions/cache and enforce the approved retention caps.
- [x] Existing writable installations remain in place; switching from an unwritable external installation requires a distinct confirmed plan action.
- [x] Fifteen focused managed-install tests cover allowlisting, single resolution, argv safety, clean verification, rollback, atomic activation, retention, cache ownership/cap, path blocking, and external-install decisions.
- [x] Coordinator inspected all four changed files and reran local formatting, metadata, and diff validation.
- [x] Final Windows and macOS workflows pass 198 Rust tests with artifact building disabled.

### Phase 4 — PATH persistence

Owner: Paseo Worker `c5ebc1e4-d6a3-4fe3-899e-354f4143eedb`
Agent workspace: isolated Paseo worktree `phase4-path-persistence`, with required rules read from the outer project
Code workspace: isolated Paseo worktree `phase4-path-persistence`
Status: Complete; the coordinator directly reviewed three Worker commits, required two focused hardening follow-ups, fixed one compiler type inference error and one cross-platform fixture-path failure, and accepted the final artifact-free platform evidence.

- [x] Implement platform adapters with injected filesystem/registry, shell-selection, and final-persistence seams for tests.
- [x] Prepend exactly one canonical managed executable path only after confirmed managed installation and preserve unrelated entries byte-for-byte where the platform format permits.
- [x] Windows updates only current-user PATH, refreshes the running process, broadcasts the change, and restores exact prior registry/process state on failure.
- [x] macOS edits only one canonical managed block for zsh/bash via same-directory atomic replacement, rejects malformed or indirect profiles, preserves permissions, and cleans temporary files without persistent backups or profile logging.
- [x] Re-probe with managed bin first plus only approved Node/npm runtime directories; expose the selected managed path and non-executed shadowed external candidates.
- [x] Coordinator inspected all seven changed files and the follow-up fixes; local formatting, locked metadata, and diff checks pass.
- [x] Final Windows workflow passes 217 Rust tests and macOS passes 226 Rust tests with artifact building disabled.

### Phase 5 — Lifecycle and UX

Owner: Paseo Worker `012c2a82-b2e1-4d2a-96de-0bb286ddbf72`
Workspace: isolated Paseo worktree `phase5-lifecycle-ux-v2`
Status: Complete; the coordinator directly reviewed three Worker commits, corrected formatting and stale frontend expectations, updated cancellation-aware Rust test fixtures, and accepted the final artifact-free platform evidence.

- [x] Extend repair action types for managed-install switching and user PATH changes; show target paths and authorization impact.
- [x] Replace mandatory checkboxes with one localized **Confirm and continue** control while retaining optional controls only where an action is genuinely optional.
- [x] Bind confirmation to server-owned task identity, allow exactly one refreshed preparation, and terminate repeated stale or mismatched confirmation with a localized state-changed result.
- [x] Enforce one globally active task and release the task claim on terminal, rejected, cancelled, and refreshed-invalidated paths.
- [x] Retry only bounded transient network failures; never retry TLS, integrity, signature, authorization, or installer failures.
- [x] Propagate cancellation through downloads and retry backoff, stop between tools, and never kill an active native MSI/PKG process.
- [x] Keep batch tool results independent and clear frontend busy/task state on every terminal outcome.
- [x] Coordinator inspected all changed backend, frontend, test, and locale files and required focused follow-up fixes for refresh and cancellation boundaries.
- [x] Final Windows workflow passes 228 Rust tests and 7 frontend test files; macOS passes 237 Rust tests and 7 frontend test files, with artifact building disabled.

### Phase 6 — Diagnostic reporting

- Produce a local structured report with deterministic redaction and home-path normalization to `~`.
- Show the exact report before transmission. The send action opens a prefilled Issue creation URL; the user remains responsible for final submission.
- Sensitive or oversized diagnostics remain local and are exported as a user-controlled file rather than placed in a public issue.

### Phase 7 — CI hardening

- Update deprecated first-party Actions and pin every third-party Action to a reviewed full SHA.
- Keep default verification manual/bounded and artifact-free: 15-minute job cap and 12-minute Rust step caps.
- Windows and Apple Silicon must pass after each accepted phase. Intel runs for release candidates or architecture-sensitive changes.
- A first failure stops the phase. Do not trigger packaging to diagnose a compile/test failure.

### Phase 8 — Acceptance, storage, and release

Windows Sandbox runtime root:

`D:\codex\ai-deploy-toolkit\test\windows-sandbox`

Rules:

- Store `input`, `runs\<run-id>`, `control`, and explicitly retained `archive` evidence under that root; never add runtime material to Git.
- Keep inputs read-only, run one scenario at a time, place all staging/evidence on D:, and refuse launch when C: has less than 20 GiB free.
- Do not enable Windows Sandbox automatically. If the feature is disabled, request separate approval before changing Windows features.
- Limit one run's evidence to 500 MiB. Retain only the latest success and latest failure under `runs`; archive only final logs, screenshots, hashes, and results, not duplicate installers.
- First release-gate scenario covers clean install, authorization, PATH refresh, and post-install execution. It does not claim all ten historical Windows scenarios.
- Preserve the existing Windows and macOS package sets and all approved ZIPs until replacement artifacts pass verification.

## Validation and progress protocol

For every implementation phase:

1. Confirm a clean authoritative worktree and exclusive file ownership.
2. Add the smallest failing test for the approved behavior where feasible.
3. Implement only that vertical slice.
4. Run formatting, focused tests, broader relevant tests, and `git diff --check`.
5. Commit the bounded change with no unrelated files.
6. Run artifact-free Windows and macOS GitHub verification with the existing hard timeouts.
7. On the first failure, inspect and fix that failure before continuing.
8. Record the commit, workflow URLs, test counts, risks, and coordinator acceptance in this document.
9. Mark a phase complete only after all acceptance evidence exists.

Packaging is allowed only after Phases 1–7 are accepted. Publishing a stable release additionally requires Phase 8's human/platform gates.

## Evidence ledger

| Date | Phase | Commit | Windows CI | macOS CI | Coordinator result | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-07-16 | Baseline | `edde7196` | `29437251201` (prior Windows baseline at `84081271`) | `29459832606` | Accepted as starting point | Both artifact-free workflows passed; Phase 1 Worker active |
| 2026-07-16 | 1 | `b4f7fbd2` | `29461965986` | `29461967206` | Accepted | Worker `bed6b7e2-5e36-4c66-9545-bed410d7c733`; Windows 6m50s, macOS 3m14s; all checks/tests passed; packaging skipped |
| 2026-07-16 | 2 | `a1a9fa72` | `29463081793` | `29463081895` | Accepted | Worker draft salvaged after provider quota failure; coordinator fixed unsafe disk-path fallback; Windows 7m19s, macOS 3m28s; all checks/tests passed; packaging skipped |
| 2026-07-16 | 3 | `478888df` | `29467359477` | `29467359532` | Accepted | Worker `b5637e36-6cc6-4a26-9975-a8d2beab02bf` produced `12dab1bb`, integrated as `fd17d61b`; coordinator fixes `0e7642fb` and `478888df`; initial compiler/test failures were stopped and fixed, one Windows registry timeout was retried once; final Windows 6m59s, macOS 3m54s; 198 Rust tests passed; packaging skipped |
| 2026-07-16 | 4 | `22e907f6` | `29472460229` | `29472461331` | Accepted | Worker `c5ebc1e4-d6a3-4fe3-899e-354f4143eedb` produced `9dfeb99a`, `ca6b0d0c`, and `095254a1`, integrated as `6e7771ab`, `26c6ec10`, and `e66cb48a`; coordinator fixes `44570771` and `22e907f6`; initial macOS compile failure and Windows fixture failures were stopped and fixed; final Windows 217 tests, macOS 226 tests; packaging skipped |
| 2026-07-16 | 5 | `8b061682` | `29481429453` | `29481431389` | Accepted | Worker `012c2a82-b2e1-4d2a-96de-0bb286ddbf72` produced `9815d482`, `d5cb619a`, and `aea4c1eb`, integrated as `e8302eba`, `1f7cec55`, and `820dceed`; coordinator fixes `138a8422`, `b87bbd42`, and `8b061682`; formatting, frontend expectation, and Rust fixture failures were fixed before acceptance; final Windows 228 Rust tests, macOS 237 Rust tests, and 7 frontend test files on both; packaging skipped |
