# Agent Manager Windows Internal Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a repeatable private-repository Windows build for Agent Manager `0.1.0` that produces an unsigned MSI, portable EXE ZIP, and SHA-256 manifest, then verify the artifacts in a disposable Windows environment without changing a real user's Agent environment.

**Architecture:** Keep the existing frontend, Rust lifecycle implementation, Tauri bundle configuration, six-Agent command/error semantics, and feature commits intact. Add one dedicated Windows GitHub Actions workflow at `.github/workflows/windows-internal.yml`; it runs all existing quality gates on `windows-2022`, builds the existing Tauri MSI, assembles the portable package from `src-tauri/target/release/agent-manager.exe`, computes `SHA256SUMS.txt`, and uploads only one Actions Artifact. Repository setup and first push are separate authority checkpoints so no command can push to `https://github.com/farion1231/cc-switch.git`.

**Tech Stack:** Git, GitHub private repository and Actions, Windows Server 2022 hosted runner, Node.js `22.12.0`, Corepack, pnpm `10.12.3`, Rust `1.95` MSVC from `rust-toolchain.toml`, Tauri CLI `2.8.x`, WiX through Tauri, PowerShell, TypeScript/Vite/Vitest, Cargo.

---

## Global Constraints

- Work from `/mnt/d/codex/ai-deploy-toolkit/apps/agent-manager`.
- Treat `docs/superpowers/specs/2026-07-12-windows-internal-build-design.md` at approved commit `e071c94` as authoritative.
- Preserve the current feature commits, including `e071c94` and its ancestors. Do not squash, rebase, amend, or reset them.
- The existing deleted files are intentional inherited upstream-document deletions. Do not restore them, and do not stage them while committing this plan.
- Keep the GitHub repository private, use `origin` for the private repository, and use `upstream` for exactly `https://github.com/farion1231/cc-switch.git`.
- Never push to `https://github.com/farion1231/cc-switch.git`; do not run `git push` until the remote URL has been inspected and confirmed to be the user-authorized private repository.
- Do not assume `gh` is installed, authenticated, or authorized to create repositories. Discovery must use `command -v gh`, `gh auth status`, and `git remote -v`; an unavailable or unauthorized `gh` stops external setup at the authority checkpoint.
- Do not add signing secrets, updater configuration, Deep Link registration, release publication, release tags, or macOS work.
- Do not put tokens, secrets, user configuration, or real Agent installation output in source, logs, or artifacts.
- Do not claim Windows or WSL results until a Windows Sandbox/VM run has actually produced evidence.
- Every implementation task ends with its focused commit before the next task begins, except external repository mutations and manual Windows verification, which require the explicit checkpoints below.

## File Map And Interfaces

- Create `.github/workflows/windows-internal.yml`: manual and target-branch-triggered Windows workflow; owns tool setup, frozen install, all gates, MSI discovery, portable ZIP assembly, SHA-256 generation, artifact upload, and failure assertions.
- Modify `src/lib/api/settings.ts` only for the current Prettier formatting failure in Task 1; preserve `settingsApi.getToolVersions`, `settingsApi.runToolLifecycleAction`, `settingsApi.probeToolInstallations`, `ToolInstallation`, and `ToolInstallationReport` signatures and field names.
- Do not modify `package.json`, `pnpm-lock.yaml`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `rust-toolchain.toml`, `src-tauri/tauri.conf.json`, or `src-tauri/tauri.windows.conf.json` unless a CI failure proves the existing configuration cannot build on Windows. Any such change requires a separate focused commit and must preserve version `0.1.0`, bundle ID `com.agentmanager.desktop`, `createUpdaterArtifacts: false`, and no deep-link configuration.
- Create `docs/superpowers/verification/2026-07-12-agent-manager-windows.md` only after a real Windows Sandbox/VM run; it records the workflow Run ID, Windows version, artifact names, SHA-256 values, each scenario result, limitations, and evidence paths.
- Artifact contract: `Agent-Manager-0.1.0-Windows.msi`, `Agent-Manager-0.1.0-Windows-Portable.zip`, and `SHA256SUMS.txt`; the uploaded artifact contains exactly these three files at its root.
- Portable ZIP contract: contain `agent-manager.exe` at the ZIP root, sourced from `src-tauri/target/release/agent-manager.exe`; do not include secrets, user data, an updater, or an installer.

## Implementation Plan

### Task 1: Fix Local Formatting And Re-run Gates

**Files:**
- Modify: `src/lib/api/settings.ts:39-47`
- Test: no new test file; existing frontend and Rust gates below

- [ ] **Step 1: Confirm the baseline failure and inspect only the reported file.**

Run from `/mnt/d/codex/ai-deploy-toolkit/apps/agent-manager`:

```bash
corepack pnpm format:check
git diff -- src/lib/api/settings.ts
```

Expected: `format:check` reports only `src/lib/api/settings.ts` as unformatted, and the diff shows no pre-existing user edit that may be overwritten. If the diff contains an unexpected user change, stop and preserve it for review instead of rewriting it.

- [ ] **Step 2: Apply the repository formatter to the single failing file.**

```bash
corepack pnpm exec prettier --write src/lib/api/settings.ts
git diff -- src/lib/api/settings.ts
```

Expected: only whitespace/layout changes required by Prettier; the three API method names, action union (`"install" | "update"`), return fields, and interface fields remain unchanged.

- [ ] **Step 3: Run all required local gates.**

```bash
corepack pnpm format:check
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: each command exits `0`; `format:check` reports all files formatted, Vitest reports passing tests, Vite writes `dist/`, and Cargo format/check/test complete without modifying tracked source. If a gate fails, diagnose and fix only the root cause before continuing; never weaken or remove the gate.

- [ ] **Step 4: Commit only the formatting fix.**

```bash
git status --short
git diff --check
git add src/lib/api/settings.ts
git diff --cached --name-status
git commit -m "style: fix settings API formatting"
```

Expected: the staged list contains only `src/lib/api/settings.ts`; the commit succeeds and the inherited deletions remain unstaged.

### Task 2: Inventory And Commit Only Inherited Upstream-Document Deletions

**Files:**
- Delete: the already-worktree-deleted upstream documents listed by `git diff --name-status --diff-filter=D`
- Modify: no feature source files

- [ ] **Step 1: Capture and review the deletion inventory before staging.**

```bash
git diff --name-status --diff-filter=D > /tmp/agent-manager-inherited-deletions.txt
git diff --name-status --diff-filter=D
git status --short
git log --oneline --decorate -12
```

Expected: every unstaged deletion is an inherited upstream document, template, image, Flatpak document, or `session-manager.md`; there are no modified/untracked feature files hidden in the deletion set. Confirm the current branch still contains the existing feature history and the Task 1 commit.

- [ ] **Step 2: Stage exactly the reviewed deletion paths, not all worktree changes.**

```bash
git diff --name-only -z --diff-filter=D | git add --pathspec-from-file=- --pathspec-file-nul
git diff --cached --name-status
git diff --cached --stat
```

Expected: every cached entry has status `D`, and the cached names exactly match `/tmp/agent-manager-inherited-deletions.txt`. If any `A`, `M`, `R`, or unexpected path appears, run `git restore --staged -- .` only for this newly created staging, re-review the inventory, and do not touch the worktree deletions.

- [ ] **Step 3: Commit the deletion boundary separately.**

```bash
git commit -m "chore: remove inherited upstream documentation"
git status --short --branch
git log --oneline -4
```

Expected: the deletion commit contains only the reviewed inherited deletions; current feature commits remain in history and the worktree is clean apart from explicitly ignored files. Do not combine workflow, verification, or unrelated source changes with this commit.

### Task 3: Prepare The Private GitHub Repository Safely

**Files:**
- Modify: local Git remote configuration only; no source file

- [ ] **Step 1: Discover available authority and current remotes without mutating anything.**

```bash
command -v gh || true
gh --version || true
gh auth status || true
git remote -v
git branch --show-current
git status --short --branch
```

Expected: the current upstream URL is visible, the current branch is known, and `gh auth status` either shows an authorized account or clearly reports that authorization is unavailable. No repository or remote is created by this step.

- [ ] **Step 2: Prepare the local remote names without contacting or pushing to a remote.**

```bash
git remote rename origin upstream
git remote set-url upstream https://github.com/farion1231/cc-switch.git
git remote -v
git ls-remote --get-url upstream
```

Expected: both fetch and push URLs for `upstream` are exactly `https://github.com/farion1231/cc-switch.git`; no `origin` exists yet and no push occurs.

- [ ] **Step 3: Stop at the explicit external-authority checkpoint.**

The implementer must ask the repository owner to confirm all of the following before continuing: the GitHub account/org, permission to create a private repository named `agent-manager`, the exact authorized private clone URL, and permission to push the current branch. After the user supplies the owner name, export it as `AUTHORIZED_OWNER` in the shell. If `gh` is authenticated, repository creation may be performed only after that confirmation:

```bash
read -r -p 'Authorized GitHub owner/org: ' AUTHORIZED_OWNER
gh repo create "$AUTHORIZED_OWNER/agent-manager" --private --source=. --remote=origin --push=false
```

If the user supplies an already-created repository instead, export its exact clone URL as `AUTHORIZED_PRIVATE_URL`; do not run `gh repo create`; configure only the supplied URL:

```bash
read -r -p 'Authorized private clone URL: ' AUTHORIZED_PRIVATE_URL
git remote add origin "$AUTHORIZED_PRIVATE_URL"
```

Expected before any push: `git remote -v` shows `origin` at the exact user-authorized private URL and `upstream` at the CCSwitch URL. Do not substitute a guessed owner, URL, account, or token. If authority or URL is missing, stop here; local workflow authoring can continue, but no external mutation is allowed.

- [ ] **Step 4: Verify repository privacy and remote safety before the first push.**

```bash
git remote get-url origin
git remote get-url upstream
gh repo view "$AUTHORIZED_OWNER/agent-manager" --json nameWithOwner,isPrivate,defaultBranchRef
git branch --show-current
git log --oneline --decorate -4
```

Expected: `origin` is the private `agent-manager` repository, `upstream` is the official CCSwitch repository, `isPrivate` is `true`, and the current feature/deletion commits are present. Do not run `git push` in this task; the push is the later explicit step after the workflow exists and the user again authorizes external mutation.

### Task 4: Add The Windows Actions Workflow And Complete Quality Gates

**Files:**
- Create: `.github/workflows/windows-internal.yml`
- Read: `package.json`, `pnpm-lock.yaml`, `.node-version`, `rust-toolchain.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`, `src-tauri/tauri.windows.conf.json`

- [ ] **Step 1: Author the workflow trigger, permissions, concurrency, and pinned tool setup.**

The workflow must use this behavior:

```yaml
name: Windows Internal Build

on:
  workflow_dispatch:
  push:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: windows-internal-${{ github.ref }}
  cancel-in-progress: true

jobs:
  build:
    runs-on: windows-2022
    steps:
      - name: Checkout
        uses: actions/checkout@v4
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: "22.12.0"
      - name: Enable pinned Corepack and pnpm
        shell: pwsh
        run: |
          $ErrorActionPreference = 'Stop'
          corepack enable
          corepack prepare pnpm@10.12.3 --activate
          if ((node --version) -ne 'v22.12.0') { throw "Unexpected Node version" }
          corepack --version
          if ((pnpm --version) -ne '10.12.3') { throw "Unexpected pnpm version" }
      - name: Setup Rust MSVC
        uses: dtolnay/rust-toolchain@1.95.0
        with:
          toolchain: 1.95.0-x86_64-pc-windows-msvc
          components: rustfmt, clippy
      - name: Verify Rust MSVC toolchain
        shell: pwsh
        run: |
          $ErrorActionPreference = 'Stop'
          rustc --version --verbose
          if ((rustup show active-toolchain) -notmatch '^1\.95\.0-x86_64-pc-windows-msvc') { throw "Unexpected Rust MSVC toolchain" }
```

Use the repository's existing `.node-version` and `rust-toolchain.toml` as the source of truth; the workflow must fail if the checked versions are not Node `22.12.0`, pnpm `10.12.3`, and the requested Rust `1.95` MSVC toolchain. Do not configure `actions/setup-node` caching; this plan intentionally omits `cache: pnpm` so setup-node cannot query a missing pnpm executable. Use the exact action references shown and do not replace tool versions with `latest` or `stable`.

- [ ] **Step 2: Add frozen dependency installation and the exact frontend/Rust gates.**

```yaml
      - name: Install dependencies
        shell: pwsh
        run: pnpm install --frozen-lockfile
      - name: Frontend format check
        run: pnpm format:check
      - name: TypeScript type check
        run: pnpm typecheck
      - name: Frontend unit tests
        run: pnpm test:unit
      - name: Renderer build
        run: pnpm build:renderer
      - name: Rust format check
        run: cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
      - name: Rust check
        run: cargo check --manifest-path src-tauri/Cargo.toml
      - name: Rust tests
        run: cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: each gate is a separate visible step and fails the job on non-zero exit. Installation uses `pnpm-lock.yaml` and never uses a mutable lockfile. Do not substitute a reduced test set, `continue-on-error`, or an environment-wide Agent install.

- [ ] **Step 3: Build the Tauri Windows MSI using the existing bundle identity.**

```yaml
      - name: Build Tauri MSI
        shell: pwsh
        run: pnpm tauri build --bundles msi
```

Expected: Tauri builds the renderer through `beforeBuildCommand`, uses version `0.1.0`, product name `Agent Manager`, bundle ID `com.agentmanager.desktop`, current `src-tauri/wix/per-user-main.wxs`, and does not create updater artifacts. Do not add signing setup or release upload permissions.

- [ ] **Step 4: Validate the workflow file locally and commit only the workflow.**

```bash
workflow=.github/workflows/windows-internal.yml
test "$(grep -E -c 'contents:[[:space:]]*write' "$workflow")" -eq 0
if grep -E -i -n 'signing|codesign|tauri_signing|gh release|action-gh-release|createUpdaterArtifacts[[:space:]]*:[[:space:]]*true|updater|deep.?link|deep_link_protocols' "$workflow"; then
  echo "Forbidden signing, release, updater, or deep-link behavior found" >&2
  exit 1
fi
expected_names=$'Agent-Manager-0.1.0-Windows.msi\nAgent-Manager-0.1.0-Windows-Portable.zip\nSHA256SUMS.txt'
actual_names=$(grep -oE 'Agent-Manager-[0-9]+\.[0-9]+\.[0-9]+-Windows(-Portable)?\.(msi|zip)|SHA256SUMS\.txt' "$workflow" | sort -u)
diff -u <(printf '%s\n' "$expected_names") <(printf '%s\n' "$actual_names")
git diff --check
git status --short
git diff -- .github/workflows/windows-internal.yml
git add .github/workflows/windows-internal.yml
git diff --cached --name-status
git commit -m "ci: add Windows internal build workflow"
```

Expected: the policy scan passes, all three exact artifact names are present in the workflow, only `.github/workflows/windows-internal.yml` is staged and committed, and the trigger, PowerShell blocks, action inputs, and expression quoting are syntactically coherent. Do not stage inherited deletions or generated `dist/`/target output.

### Task 5: Package Portable ZIP, SHA256SUMS, And Actions Artifact Only

**Files:**
- Modify: `.github/workflows/windows-internal.yml`

- [ ] **Step 1: Add deterministic MSI discovery and portable ZIP assembly.**

Add a PowerShell step after the Tauri build:

```yaml
      - name: Assemble internal artifacts
        shell: pwsh
        run: |
          $ErrorActionPreference = 'Stop'
          $artifactDir = Join-Path $env:GITHUB_WORKSPACE 'internal-artifacts'
          New-Item -ItemType Directory -Force -Path $artifactDir | Out-Null

          $msis = @(Get-ChildItem 'src-tauri/target/release/bundle/msi/*.msi' -File)
          if ($msis.Count -ne 1) { throw "Expected exactly one MSI, found $($msis.Count)" }
          Copy-Item $msis[0].FullName (Join-Path $artifactDir 'Agent-Manager-0.1.0-Windows.msi')

          $portableExe = 'src-tauri/target/release/agent-manager.exe'
          if (-not (Test-Path $portableExe -PathType Leaf)) { throw "Missing $portableExe" }
          $portableStage = Join-Path $env:RUNNER_TEMP 'agent-manager-portable'
          Remove-Item $portableStage -Recurse -Force -ErrorAction SilentlyContinue
          New-Item -ItemType Directory -Force -Path $portableStage | Out-Null
          Copy-Item $portableExe (Join-Path $portableStage 'agent-manager.exe')
          Compress-Archive -Path (Join-Path $portableStage 'agent-manager.exe') -DestinationPath (Join-Path $artifactDir 'Agent-Manager-0.1.0-Windows-Portable.zip') -CompressionLevel Optimal

          $files = @(
            'Agent-Manager-0.1.0-Windows.msi',
            'Agent-Manager-0.1.0-Windows-Portable.zip'
          )
          $lines = foreach ($file in $files) {
            $hash = (Get-FileHash (Join-Path $artifactDir $file) -Algorithm SHA256).Hash.ToLowerInvariant()
            "$hash  $file"
          }
          $lines | Set-Content (Join-Path $artifactDir 'SHA256SUMS.txt') -Encoding ascii
          Get-ChildItem $artifactDir | Format-Table Name,Length
```

Expected: the step fails if the MSI is missing/ambiguous, the EXE is missing, either archive is empty, or checksum generation fails. `SHA256SUMS.txt` contains exactly two lowercase SHA-256 lines, one for each binary artifact, and no secrets or user configuration.

- [ ] **Step 2: Verify artifact names, non-empty files, ZIP contents, and checksums before upload.**

```yaml
      - name: Verify internal artifacts
        shell: pwsh
        run: |
          $ErrorActionPreference = 'Stop'
          $artifactDir = Join-Path $env:GITHUB_WORKSPACE 'internal-artifacts'
          $expected = @(
            'Agent-Manager-0.1.0-Windows.msi',
            'Agent-Manager-0.1.0-Windows-Portable.zip',
            'SHA256SUMS.txt'
          )
          $actual = @(Get-ChildItem $artifactDir -File | Select-Object -ExpandProperty Name)
          if ((Compare-Object $expected $actual)) { throw "Artifact names do not match contract" }
           foreach ($name in $expected) {
             if ((Get-Item (Join-Path $artifactDir $name)).Length -le 0) { throw "Empty artifact: $name" }
           }
           $zipEntries = @(tar -tf (Join-Path $artifactDir 'Agent-Manager-0.1.0-Windows-Portable.zip'))
           if ($zipEntries.Count -ne 1 -or $zipEntries[0] -ne 'agent-manager.exe') { throw "Portable ZIP must contain only agent-manager.exe at its root" }
           $manifestLines = @(Get-Content (Join-Path $artifactDir 'SHA256SUMS.txt'))
           if ($manifestLines.Count -ne 2) { throw "SHA256SUMS.txt must contain exactly two lines" }
           $manifest = @{}
           foreach ($line in $manifestLines) {
             if ($line -cnotmatch '^(?<hash>[0-9a-f]{64})  (?<name>.+)$') { throw "Malformed SHA256SUMS.txt line: $line" }
             $name = $Matches.name
             if (-not ($expected | Where-Object { $_ -ceq $name })) { throw "Unexpected checksum filename: $name" }
             if ($manifest.ContainsKey($name)) { throw "Duplicate checksum filename: $name" }
             $manifest[$name] = $Matches.hash
           }
           foreach ($name in $expected) {
             if (-not $manifest.ContainsKey($name)) { throw "Missing checksum filename: $name" }
           }
           foreach ($name in $expected[0..1]) {
             $freshHash = (Get-FileHash (Join-Path $artifactDir $name) -Algorithm SHA256).Hash.ToLowerInvariant()
             if ($manifest[$name] -cne $freshHash) { throw "Checksum mismatch for $name" }
           }
```

Expected: exact three-file output, all non-empty, portable ZIP contains only the root EXE, and verification fails for any extra/missing manifest line, malformed or uppercase hash, unexpected/duplicate filename, missing expected filename, or mismatch against fresh `Get-FileHash` results.

- [ ] **Step 3: Upload only an Actions Artifact and commit the packaging change.**

```yaml
      - name: Upload internal build artifact
        uses: actions/upload-artifact@v4
        with:
          name: Agent-Manager-0.1.0-Windows-internal
          path: internal-artifacts/
          if-no-files-found: error
          retention-days: 7
```

The workflow must not grant `contents: write`, call `gh release`, invoke `softprops/action-gh-release`, create tags, or publish to a package/release registry.

```bash
git diff --check
git add .github/workflows/windows-internal.yml
git diff --cached --name-status
git commit -m "ci: package Windows internal artifacts"
```

Expected: only the workflow is staged; the commit adds artifact packaging without signing or release behavior.

### Task 6: Push, Trigger, And Diagnose The First Windows Build

**Files:**
- Modify: none unless a CI-proven defect requires a focused follow-up commit

- [ ] **Step 1: Re-check authority and remotes immediately before pushing.**

```bash
git status --short --branch
git remote get-url origin
git remote get-url upstream
git log --oneline --decorate -8
gh repo view "$AUTHORIZED_OWNER/agent-manager" --json nameWithOwner,isPrivate,defaultBranchRef
```

Expected: worktree is clean, `origin` is the confirmed private repository, `upstream` is the official CCSwitch URL, repository privacy is `true`, and all implementation commits are present. If any check differs, stop and request renewed user authorization; never push to `upstream`.

- [ ] **Step 2: Push only the authorized branch and wait for its push-triggered run.**

```bash
HEAD_SHA=$(git rev-parse HEAD)
git push --set-upstream origin main
RUN_ID=''
for attempt in {1..12}; do
  RUN_ID=$(gh run list --repo "$AUTHORIZED_OWNER/agent-manager" --workflow windows-internal.yml --commit "$HEAD_SHA" --limit 1 --json databaseId --jq '.[0].databaseId // empty')
  if [ -n "$RUN_ID" ]; then break; fi
  sleep 5
done
if [ -z "$RUN_ID" ]; then
  gh workflow run "Windows Internal Build" --repo "$AUTHORIZED_OWNER/agent-manager" --ref main
  RUN_ID=$(gh run list --repo "$AUTHORIZED_OWNER/agent-manager" --workflow windows-internal.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId // empty')
fi
if [ -z "$RUN_ID" ]; then echo "Unable to identify a Windows workflow run" >&2; exit 1; fi
printf 'Windows workflow run: %s\n' "$RUN_ID"
```

Expected: the push destination is the private `origin`, and the identified run has `headSha` equal to `$HEAD_SHA`. The push-triggered run is used by default; `gh workflow run` executes only after the lookup timeout finds no run, or later when explicitly retrying a failed build. No release is created. If the branch is not `main`, use the actual branch from `git branch --show-current` consistently for push, `--commit`, `--branch`, and `--ref`.

- [ ] **Step 3: Inspect the run and retain failure evidence without weakening gates.**

```bash
gh run watch "$RUN_ID" --repo "$AUTHORIZED_OWNER/agent-manager" --exit-status
gh run view "$RUN_ID" --repo "$AUTHORIZED_OWNER/agent-manager" --log-failed
gh run view "$RUN_ID" --repo "$AUTHORIZED_OWNER/agent-manager" --json conclusion,status,headSha,workflowName,url
```

Expected on success: conclusion `success`, all quality gates pass, MSI/ZIP/checksum verification passes, and the Actions Artifact is downloadable. On failure, classify the failure as tool setup, dependency/cache, frontend gate, Rust/MSVC, Tauri/WiX, packaging, or upload; fix the smallest root cause in a focused commit, rerun the same gates, and trigger a new run. Never use `continue-on-error`, skip a gate, install Agents globally on the runner, or accept a missing/partial artifact.

### Task 7: Execute The Disposable Windows Sandbox/VM Checklist

**Files:**
- Create: `docs/superpowers/verification/2026-07-12-agent-manager-windows.md` after testing

- [ ] **Step 1: Provision and document a disposable environment.**

Record Windows edition/build, Sandbox or VM identifier, WebView2 Runtime presence, WSL availability, workflow Run ID, artifact download time, and SHA-256 values before installing anything. Download the Actions Artifact into a disposable directory, verify `SHA256SUMS.txt` with:

```powershell
$expected = @('Agent-Manager-0.1.0-Windows.msi', 'Agent-Manager-0.1.0-Windows-Portable.zip')
$lines = @(Get-Content .\SHA256SUMS.txt)
if ($lines.Count -ne $expected.Count) { throw 'SHA256SUMS.txt must contain exactly two lines' }
$manifest = @{}
foreach ($line in $lines) {
  if ($line -cnotmatch '^(?<hash>[0-9a-f]{64})  (?<name>.+)$') { throw "Malformed checksum line: $line" }
  $name = $Matches.name
  if (-not ($expected | Where-Object { $_ -ceq $name })) { throw "Unexpected checksum filename: $name" }
  if ($manifest.ContainsKey($name)) { throw "Duplicate checksum filename: $name" }
  $manifest[$name] = $Matches.hash
}
foreach ($name in $expected) {
  if (-not $manifest.ContainsKey($name)) { throw "Missing checksum filename: $name" }
  $freshHash = (Get-FileHash ".\$name" -Algorithm SHA256).Hash.ToLowerInvariant()
  if ($manifest[$name] -cne $freshHash) { throw "Checksum mismatch for $name" }
}
```

Expected: both lowercase hashes and both exact filenames match fresh file hashes; malformed, uppercase, duplicate, extra, or missing manifest entries fail. Do not copy artifacts to the host, use a personal profile, or run the MSI/EXE outside the disposable environment.

- [ ] **Step 2: Verify MSI install, launch, and uninstall without host mutation.**

Run `msiexec /i .\Agent-Manager-0.1.0-Windows.msi /passive`, launch Agent Manager from the installed Start Menu entry, and confirm the window opens. Record the per-user install location, shortcuts, and process exit behavior. Close the app and run `msiexec /x .\Agent-Manager-0.1.0-Windows.msi /passive`; confirm the installed files, shortcuts, and per-user registration are removed. Check that no machine-wide service, scheduled updater, Deep Link protocol, or CCSwitch updater source appears.

Expected: MSI install/start/uninstall pass, unsigned SmartScreen/unknown-publisher warnings are recorded as known limitations rather than failures, and uninstall does not modify the host because all actions occurred in the disposable environment.

- [ ] **Step 3: Verify portable launch and runtime prerequisites.**

Extract `Agent-Manager-0.1.0-Windows-Portable.zip` to a fresh disposable directory and launch `.\agent-manager.exe`. Record whether WebView2 Runtime was already present and whether the portable build starts without installation. Do not add the EXE to the real user's PATH or copy it into a global Agent directory.

Expected: portable app launches in the disposable environment, or the report records a reproducible WebView2 prerequisite failure and does not call the candidate accepted.

- [ ] **Step 4: Verify all six Agent lifecycle states and operations.**

Use the UI's six configured tools (Claude Code, Codex, Gemini CLI, OpenCode, OpenClaw, and Hermes) and record a matrix with columns `tool`, `environment`, `state`, `probe result`, `action`, `result`, and `evidence`. For each tool, exercise `not installed`, `installed and runnable`, `installed_but_broken`, and `upgrade available`; verify the UI preserves the `get_tool_versions`/`probe_tool_installations` fields and error semantics. Run one-item install, one-item upgrade, batch install/upgrade, and a batch where one simulated/controlled command fails; confirm remaining items continue and the failed item retains an actionable error.

Real installer commands are allowed only inside the disposable Sandbox/VM. When real tool or network execution cannot be performed, use the repository's unit tests, command-plan behavior, and simulated executables for the non-destructive path, and mark the real installation scenario `blocked` rather than fabricating a pass.

Expected: all six tools are represented; lifecycle state detection, single/batch operations, and failure isolation match the existing implementation. No real user Agent environment is changed.

- [ ] **Step 5: Verify PATH conflicts, install locations, native Shell, and WSL forwarding.**

Create two disposable install locations for at least one tool, put one on the disposable PATH, and use the diagnostic flow to confirm multiple installations, `is_path_default`, source/path, runnable state, and anchored/unanchored upgrade messaging. Test Windows-native shell arguments and, only if the VM has WSL enabled, each supported WSL shell/flag combination used by `WSL_SHELL_OPTIONS` and `WSL_SHELL_FLAG_OPTIONS` in `src/components/AgentLifecyclePage.tsx`. If Sandbox lacks WSL, repeat this one section in a separate disposable VM.

Expected: PATH conflict diagnostics identify the command-line default and do not silently claim all installations were upgraded; native Windows and WSL argument forwarding are recorded separately, with unavailable WSL marked as an environment limitation.

- [ ] **Step 6: Check excluded side effects and export evidence before teardown.**

Inspect application/network logs and Windows registrations for requests to CCSwitch update sources, updater artifacts, Deep Link registrations, or excluded background services. Export screenshots, command output, and logs to a safe evidence directory before closing Sandbox/VM. Never include credentials, personal configuration, or tokens in the evidence.

Expected: no CCSwitch update-source access, Deep Link registration, or excluded service startup is observed. Any unexpected side effect immediately invalidates the candidate and stops acceptance.

### Task 8: Write Windows Verification Evidence And Candidate Acceptance

**Files:**
- Create: `docs/superpowers/verification/2026-07-12-agent-manager-windows.md`

- [ ] **Step 1: Write the evidence report from observed results only.**

Use this concrete report structure:

```markdown
# Agent Manager Windows Internal Build Verification

- Run ID and URL:
- Commit SHA:
- Windows edition/build:
- Sandbox or VM:
- WebView2 status:
- WSL status and environment:
- Artifact names:
- SHA-256 values:

## Results

| Scenario | Result | Evidence |
| --- | --- | --- |
| MSI install/start/uninstall | PASS/FAIL/BLOCKED | path |
| Portable EXE start | PASS/FAIL/BLOCKED | path |
| Six-Agent state matrix | PASS/FAIL/BLOCKED | path |
| Single install/upgrade | PASS/FAIL/BLOCKED | path |
| Batch continuation after failure | PASS/FAIL/BLOCKED | path |
| PATH/install-location conflict diagnosis | PASS/FAIL/BLOCKED | path |
| Windows shell forwarding | PASS/FAIL/BLOCKED | path |
| WSL shell forwarding | PASS/FAIL/BLOCKED | path |
| Excluded side-effect checks | PASS/FAIL/BLOCKED | path |

## Known Limitations

- Unsigned binaries may show Unknown Publisher or SmartScreen warnings.
- WebView2 and WSL results are environment-dependent and are listed only as observed.

## Candidate Decision

ACCEPTED only if every required scenario is PASS, all three artifacts and hashes match, and no excluded side effect is observed. Otherwise REJECTED or BLOCKED with the exact failure and next action.
```

Expected: no placeholder values remain; `PASS`, `FAIL`, and `BLOCKED` values correspond to actual evidence, and the report does not fabricate Windows results.

- [ ] **Step 2: Run final local/document checks and commit only the report.**

```bash
git diff --check
git status --short
git add docs/superpowers/verification/2026-07-12-agent-manager-windows.md
git diff --cached --name-status
git commit -m "docs: record Windows internal build verification"
```

Expected: only the verification report is staged. If a required scenario is blocked, commit the accurately marked report but do not label the package an accepted candidate.

## Acceptance Criteria

- Local `format:check`, `typecheck`, `test:unit`, `build:renderer`, Cargo format, Cargo check, and Cargo test pass.
- Inherited upstream-document deletions are one intentional standalone commit and are not mixed with feature or workflow commits.
- The private repository and remote mapping are user-authorized and verified: `origin` is private `agent-manager`; `upstream` is exactly `https://github.com/farion1231/cc-switch.git`.
- The Windows workflow is manually triggerable, target-branch triggerable, uses frozen dependencies, pins Node/Corepack/pnpm/Rust MSVC, and exposes every required quality gate.
- A successful Windows run contains exactly `Agent-Manager-0.1.0-Windows.msi`, `Agent-Manager-0.1.0-Windows-Portable.zip`, and `SHA256SUMS.txt` in one Actions Artifact; no Release or signing occurs.
- MSI and portable EXE start in a disposable Windows environment; six-Agent lifecycle, batch failure isolation, PATH conflict, native shell, and available WSL scenarios have evidence or are explicitly blocked.
- No updater, Deep Link, excluded service, or CCSwitch update-source access is observed.
- The verification report records Run ID, platform, hashes, scenario results, limitations, and evidence locations.

## Verification Commands Summary

```bash
corepack pnpm format:check
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
git status --short --branch
git log --oneline --decorate -12
git remote -v
```

The Windows workflow repeats the same seven quality gates, then asserts the exact MSI/ZIP/checksum contract before `actions/upload-artifact@v4`. Manual Windows verification is not replaced by Linux/WSL cross-compilation.
