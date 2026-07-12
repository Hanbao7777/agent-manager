# Agent Manager macOS Universal Internal Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Agent Manager `0.1.0` as one unsigned Universal DMG supporting Apple Silicon and Intel, upload it only as a strictly verified private Actions Artifact, and record real-Mac verification honestly.

**Architecture:** Add a macOS-only workflow beside the Windows workflow. A native `macos-14` runner installs both Apple Rust targets, runs all current gates, builds `universal-apple-darwin`, verifies the app with `lipo`, normalizes the single DMG, validates a one-line SHA-256 manifest, and uploads exactly two files. CI acceptance and real-Mac acceptance remain separate.

**Tech Stack:** GitHub Actions, `macos-14`, Node `22.12.0`, Corepack `0.31.0`, pnpm `10.12.3`, Rust `1.95`, Tauri `2.8.x`, `aarch64-apple-darwin`, `x86_64-apple-darwin`, `lipo`, `shasum`.

## Global Constraints

- Work from `/mnt/d/codex/ai-deploy-toolkit/apps/agent-manager`.
- The approved spec is `docs/superpowers/specs/2026-07-12-macos-universal-internal-build-design.md` at `fa2bd08c`.
- Preserve `Agent Manager`, version `0.1.0`, identifier `com.agentmanager.desktop`, and `createUpdaterArtifacts: false`.
- `origin` remains private `https://github.com/Hanbao7777/agent-manager.git`; `upstream` remains exactly `https://github.com/farion1231/cc-switch.git`; never push upstream.
- Do not modify `.github/workflows/windows-internal.yml` or its artifact contract.
- Do not add signing, notarization, Apple secrets, Release/tags, updater, Deep Link, App Store, or `.pkg` behavior.
- Preserve the six-Agent lifecycle implementation and error semantics.
- Artifact name: `Agent-Manager-0.1.0-macOS-Universal-internal`, retained 7 days.
- Artifact root: exactly non-empty `Agent-Manager-0.1.0-macOS-Universal.dmg` and `SHA256SUMS.txt`.
- Manifest: exactly one lowercase SHA-256 line for the exact DMG filename.
- The executable must contain the exact sorted architecture set `arm64 x86_64`; renaming a single-architecture DMG is forbidden.
- CI fixes require failure-log evidence, focused commits, and may not weaken gates.

## File Map And Interfaces

- Create `.github/workflows/macos-internal.yml`: all macOS setup, gates, Universal build, architecture checks, packaging, checksum validation, and upload.
- Read only unless CI proves otherwise: `.node-version`, `rust-toolchain.toml`, `package.json`, lockfiles, Tauri manifests/config.
- Create `docs/superpowers/verification/2026-07-12-agent-manager-macos.md` from observed evidence only.
- Ignored handoffs: `.superpowers/sdd/macos-task-N-report.md`.

---

### Task 1: Audit Inputs And Baseline Gates

**Files:**
- Modify: none
- Report: `.superpowers/sdd/macos-task-1-report.md`

**Interfaces:**
- Consumes: current repository configuration.
- Produces: exact version/config inventory and baseline evidence for workflow authoring.

- [ ] **Step 1: Verify repository and build identity.**

```bash
git status --short --branch
git remote -v
git log --oneline --decorate -8
cat .node-version
cat rust-toolchain.toml
node -e 'const p=require("./package.json"); console.log(p.devDependencies["@tauri-apps/cli"],p.scripts)'
node -e 'const c=require("./src-tauri/tauri.conf.json"); console.log(c.productName,c.version,c.identifier,c.bundle.createUpdaterArtifacts,c.bundle.macOS.minimumSystemVersion)'
```

Expected: Node `22.12.0`, Rust `1.95`, Tauri `^2.8.0`, product/version/identifier as constrained, updater artifacts false, minimum macOS `12.0`, and correct remotes.

- [ ] **Step 2: Run all portable gates.**

```bash
corepack pnpm format:check
corepack pnpm typecheck
corepack pnpm test:unit
corepack pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all exit `0`. Report exact test counts and explicitly state Linux does not prove native macOS packaging.

- [ ] **Step 3: Write ignored evidence and prove read-only scope.**

```bash
git check-ignore -v .superpowers/sdd/macos-task-1-report.md
git diff --check
git status --short --branch
```

Expected: no tracked implementation change. Task 1 has no commit.

### Task 2: Add Toolchain, Gates, And Universal Build

**Files:**
- Create: `.github/workflows/macos-internal.yml`
- Report: `.superpowers/sdd/macos-task-2-report.md`

**Interfaces:**
- Consumes: Task 1 versions.
- Produces: a workflow that creates one verified Universal app and one raw DMG.

- [ ] **Step 1: Create pinned workflow setup.**

```yaml
name: macOS Universal Internal Build

on:
  workflow_dispatch:
  push:
    branches: [main]

permissions:
  contents: read

concurrency:
  group: macos-universal-internal-${{ github.ref }}
  cancel-in-progress: true

jobs:
  build:
    runs-on: macos-14
    steps:
      - name: Checkout
        uses: actions/checkout@v4
      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: "22.12.0"
      - name: Enable pinned Corepack and pnpm
        shell: bash
        run: |
          set -euo pipefail
          npm install --global --force corepack@0.31.0
          corepack enable
          test "$(corepack --version)" = "0.31.0"
          corepack prepare pnpm@10.12.3 --activate
          test "$(node --version)" = "v22.12.0"
          test "$(pnpm --version)" = "10.12.3"
      - name: Setup Rust Apple targets
        uses: dtolnay/rust-toolchain@1.95.0
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin
          components: rustfmt,clippy
      - name: Verify Rust toolchain and targets
        shell: bash
        run: |
          set -euo pipefail
          test "$(rustc --version | awk '{print $2}')" = "1.95.0"
          rustup target list --installed | sort > /tmp/targets.txt
          grep -Fx aarch64-apple-darwin /tmp/targets.txt
          grep -Fx x86_64-apple-darwin /tmp/targets.txt
```

Expected: read-only permissions, no setup-node pnpm cache, and no floating versions.

- [ ] **Step 2: Add all current quality gates.**

```yaml
      - name: Install dependencies
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

Expected: no `continue-on-error`; any failure prevents packaging.

- [ ] **Step 3: Build and prove a real Universal app.**

```yaml
      - name: Build Tauri Universal DMG
        run: pnpm tauri build --target universal-apple-darwin --bundles dmg
      - name: Verify Universal app architecture
        shell: bash
        run: |
          set -euo pipefail
          apps=()
          while IFS= read -r path; do apps+=("$path"); done < <(find src-tauri/target/universal-apple-darwin/release/bundle/macos -maxdepth 1 -type d -name '*.app' -print)
          test "${#apps[@]}" -eq 1
          executable="${apps[0]}/Contents/MacOS/agent-manager"
          test -s "$executable"
          actual="$(lipo -archs "$executable" | tr ' ' '\n' | sort | tr '\n' ' ' | sed 's/ $//')"
          test "$actual" = 'arm64 x86_64'
          file "$executable"
```

Expected: exactly one app, non-empty main executable, exact two-architecture set.

- [ ] **Step 4: Validate policy and commit only the workflow.**

```bash
python3 - <<'PY'
from pathlib import Path
import yaml
d=yaml.safe_load(Path('.github/workflows/macos-internal.yml').read_text())
assert d['permissions']=={'contents':'read'}
assert d['jobs']['build']['runs-on']=='macos-14'
PY
if rg -n 'contents:\s*write|gh release|action-gh-release|notary|codesign|APPLE_|updater|deep.?link|continue-on-error' .github/workflows/macos-internal.yml; then exit 1; fi
git diff --check
git add .github/workflows/macos-internal.yml
git diff --cached --name-status
git commit -m "ci: add macOS universal internal build"
```

Expected: only the new workflow is committed.

### Task 3: Add Strict DMG And Artifact Contract

**Files:**
- Modify: `.github/workflows/macos-internal.yml`
- Report: `.superpowers/sdd/macos-task-3-report.md`

**Interfaces:**
- Consumes: Task 2 raw DMG.
- Produces: exact two-file private Artifact contract.

- [ ] **Step 1: Add deterministic assembly.**

```yaml
      - name: Assemble internal artifact
        shell: bash
        run: |
          set -euo pipefail
          artifact_dir="$GITHUB_WORKSPACE/internal-artifacts"
          rm -rf "$artifact_dir" && mkdir -p "$artifact_dir"
          dmgs=()
          while IFS= read -r path; do dmgs+=("$path"); done < <(find src-tauri/target/universal-apple-darwin/release/bundle/dmg -maxdepth 1 -type f -name '*.dmg' -print)
          test "${#dmgs[@]}" -eq 1
          name='Agent-Manager-0.1.0-macOS-Universal.dmg'
          cp "${dmgs[0]}" "$artifact_dir/$name"
          test -s "$artifact_dir/$name"
          hash="$(shasum -a 256 "$artifact_dir/$name" | awk '{print tolower($1)}')"
          printf '%s  %s\n' "$hash" "$name" > "$artifact_dir/SHA256SUMS.txt"
```

- [ ] **Step 2: Add exact name/manifest/fresh-hash assertions.**

```yaml
      - name: Verify internal artifact
        shell: bash
        run: |
          set -euo pipefail
          dir="$GITHUB_WORKSPACE/internal-artifacts"
          name='Agent-Manager-0.1.0-macOS-Universal.dmg'
          actual="$(find "$dir" -maxdepth 1 -type f -exec basename {} \; | sort | tr '\n' ' ' | sed 's/ $//')"
          test "$actual" = "$name SHA256SUMS.txt"
          test -s "$dir/$name" && test -s "$dir/SHA256SUMS.txt"
          lines="$(wc -l < "$dir/SHA256SUMS.txt" | tr -d ' ')"
          test "$lines" = 1
          line="$(cat "$dir/SHA256SUMS.txt")"
          [[ "$line" =~ ^([0-9a-f]{64})\ \ (Agent-Manager-0\.1\.0-macOS-Universal\.dmg)$ ]]
          fresh="$(shasum -a 256 "$dir/$name" | awk '{print tolower($1)}')"
          test "${BASH_REMATCH[1]}" = "$fresh"
```

- [ ] **Step 3: Upload and commit.**

```yaml
      - name: Upload internal build artifact
        uses: actions/upload-artifact@v4
        with:
          name: Agent-Manager-0.1.0-macOS-Universal-internal
          path: internal-artifacts/
          if-no-files-found: error
          retention-days: 7
```

```bash
test "$(rg -c 'uses: actions/upload-artifact@v4' .github/workflows/macos-internal.yml)" = 1
git diff --check
git add .github/workflows/macos-internal.yml
git diff --cached --name-status
git commit -m "ci: package macOS universal internal artifact"
```

Expected: only workflow packaging changes; no signing/release behavior.

### Task 4: Push And Diagnose The Native macOS Run

**Files:**
- Modify: none unless CI proves a focused defect
- Report: `.superpowers/sdd/macos-task-4-report.md`

**Interfaces:**
- Consumes: committed workflow and authorized private origin.
- Produces: successful exact-HEAD Run ID/URL and independently verified Artifact.

- [ ] **Step 1: Re-check external authority.**

```bash
git status --short --branch
git remote get-url origin
git remote get-url upstream
gh auth status
gh repo view Hanbao7777/agent-manager --json nameWithOwner,isPrivate,defaultBranchRef
HEAD_SHA=$(git rev-parse HEAD)
```

Expected: clean `main`, private origin, exact official upstream, authenticated account. Stop on mismatch.

- [ ] **Step 2: Push origin only and discover the exact run.**

```bash
git push origin main
RUN_ID=''
for attempt in {1..12}; do
  RUN_ID=$(gh run list --repo Hanbao7777/agent-manager --workflow macos-internal.yml --commit "$HEAD_SHA" --limit 1 --json databaseId --jq '.[0].databaseId // empty')
  test -n "$RUN_ID" && break
  sleep 5
done
if test -z "$RUN_ID"; then
  gh workflow run 'macOS Universal Internal Build' --repo Hanbao7777/agent-manager --ref main
  RUN_ID=$(gh run list --repo Hanbao7777/agent-manager --workflow macos-internal.yml --branch main --limit 1 --json databaseId --jq '.[0].databaseId // empty')
fi
test -n "$RUN_ID"
```

Expected: push-triggered run preferred; manual run only after timeout.

- [ ] **Step 3: Watch and diagnose without weakening gates.**

```bash
gh run watch "$RUN_ID" --repo Hanbao7777/agent-manager --exit-status
gh run view "$RUN_ID" --repo Hanbao7777/agent-manager --json conclusion,status,headSha,workflowName,url,event
gh run view "$RUN_ID" --repo Hanbao7777/agent-manager --log-failed
```

Expected: success on exact pushed HEAD. For failure, classify setup/frontend/Rust/universal/DMG/checksum/upload, make the smallest log-proven focused fix, rerun relevant local gates, push origin only, and repeat.

- [ ] **Step 4: Independently download and verify.**

```bash
rm -rf /tmp/agent-manager-macos-artifact && mkdir /tmp/agent-manager-macos-artifact
gh run download "$RUN_ID" --repo Hanbao7777/agent-manager --name Agent-Manager-0.1.0-macOS-Universal-internal --dir /tmp/agent-manager-macos-artifact
cd /tmp/agent-manager-macos-artifact
test "$(find . -maxdepth 1 -type f | wc -l)" -eq 2
test -s Agent-Manager-0.1.0-macOS-Universal.dmg
test -s SHA256SUMS.txt
shasum -a 256 -c SHA256SUMS.txt
```

Expected: exactly two files and passing checksum. Report final SHA, run/artifact IDs, URLs, sizes, expiry, commits, and failed-run history.

### Task 5: Real-Mac Verification Report

**Files:**
- Create: `docs/superpowers/verification/2026-07-12-agent-manager-macos.md`

**Interfaces:**
- Consumes: Task 4 Artifact and real Mac/VM observations.
- Produces: `ACCEPTED`, `REJECTED`, or `BLOCKED` decision.

- [ ] **Step 1: Verify DMG and Universal binary on disposable macOS.**

```bash
shasum -a 256 -c SHA256SUMS.txt
hdiutil attach Agent-Manager-0.1.0-macOS-Universal.dmg -nobrowse
lipo -archs '/Volumes/Agent Manager/Agent Manager.app/Contents/MacOS/agent-manager'
spctl --assess --type execute --verbose=4 '/Volumes/Agent Manager/Agent Manager.app' || true
```

Expected: checksum, mount, and dual architecture pass; unsigned Gatekeeper output is recorded exactly.

- [ ] **Step 2: Test in a disposable user/VM.**

Record app copy/launch after explicit Gatekeeper allowance, all six cards, not-installed and runnable detection, one controlled install/update, batch failure continuation, multi-install/PATH conflict diagnosis, and absence of updater/Deep Link/excluded services. Real Agent commands must not touch a real user profile. Unavailable cases are `BLOCKED`.

- [ ] **Step 3: Write exact observed evidence.**

The report must include Run ID/URL, commit, macOS build, hardware architecture, VM/real Mac identity, artifact names/hash, evidence paths, known unsigned limitation, and a table for checksum, DMG mount, Universal binary, Gatekeeper/launch, six-Agent UI/detection, single action, batch isolation, PATH conflict, and excluded side effects.

- [ ] **Step 4: Reject placeholders and commit only the report.**

```bash
if rg -n 'TBD|TODO|PLACEHOLDER|PASS/FAIL/BLOCKED|path \|' docs/superpowers/verification/2026-07-12-agent-manager-macos.md; then exit 1; fi
git diff --check
git add docs/superpowers/verification/2026-07-12-agent-manager-macos.md
git diff --cached --name-status
git commit -m "docs: record macOS universal build verification"
```

Expected: only the report is committed. Without real macOS evidence, decision is `BLOCKED`, never `ACCEPTED`.

## Acceptance Criteria

- Native macOS workflow passes all seven current frontend/Rust gates.
- Exact-HEAD run proves the executable contains `arm64 x86_64`.
- One private 7-day Artifact contains only the normalized DMG and one-line lowercase checksum.
- Independent download reproduces SHA-256.
- No Windows workflow, signing, notarization, Release, updater, Deep Link, or Apple secret change occurs.
- Real Mac/VM evidence records all required scenarios honestly; unavailable cases remain `BLOCKED`.

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
git remote -v
```
