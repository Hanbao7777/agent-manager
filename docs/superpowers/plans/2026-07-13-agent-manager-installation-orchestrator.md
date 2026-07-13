# Agent Manager Installation Orchestrator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete Windows/macOS installation flow inside Agent Manager that detects and repairs Node.js/npm prerequisites, requests authorization for system changes, installs Agent CLIs, and verifies that each CLI is actually runnable.

**Architecture:** Add a focused Rust `installer` module with a serializable task model, environment probe, repair planner, platform adapters, tool strategies, failure classification, and an orchestrator. Keep WSL and the existing update path stable, expose install preflight/execute commands through Tauri, and reuse the current lifecycle page, dialogs, progress styling, and diagnostics UI.

**Tech Stack:** Rust 1.85, Tokio, Reqwest/rustls, Serde, Tauri 2 events/commands, React 18, TypeScript, Vitest, Testing Library, i18next.

## Global Constraints

- Cover Windows native and macOS; preserve the existing WSL path without adding WSL dependency repair.
- Preview and confirm every Node.js installation, system-level PATH change, MSI/PKG invocation, system-directory permission change, or system tool replacement.
- Never uninstall unknown Node.js/CLI installations, delete global npm packages, force-close user processes, weaken security settings, or bypass signature/TLS checks.
- Prefer official Node.js packages and verify HTTPS source, SHA-256, platform signature, and CPU architecture before execution.
- Reuse `AgentLifecyclePage`, current cards, dialog primitives, progress styles, batch summaries, and install-location diagnostics; add only the minimum new UI required for preflight, authorization, progress, and structured results.
- Treat installation as successful only after resolving the actual command path and obtaining a valid version from a clean child-process environment.
- Keep logs useful but redact API keys, tokens, proxy credentials, sensitive URL components, and known secret environment variables.
- Use a shared dependency preflight for batch installs; isolate each tool result so one failure does not stop the remaining tools.

---

## File Map

**Create:**

- `src-tauri/src/installer/mod.rs` — public module surface and Tauri command exports.
- `src-tauri/src/installer/model.rs` — serializable task, probe, plan, event, result, and error types.
- `src-tauri/src/installer/policy.rs` — supported Node.js policy and tool metadata.
- `src-tauri/src/installer/failure.rs` — stable failure classification and redaction.
- `src-tauri/src/installer/probe.rs` — command/environment/network/disk probe abstraction.
- `src-tauri/src/installer/repair.rs` — pure repair-plan generation.
- `src-tauri/src/installer/platform/mod.rs` — platform adapter trait and current adapter selection.
- `src-tauri/src/installer/platform/windows.rs` — Windows Node MSI, Authenticode, UAC, and PATH refresh behavior.
- `src-tauri/src/installer/platform/macos.rs` — macOS Node PKG, signature, authorization, and GUI/login PATH behavior.
- `src-tauri/src/installer/tools.rs` — tool install strategies and postflight verification metadata.
- `src-tauri/src/installer/orchestrator.rs` — task state machine and batch failure isolation.
- `src/lib/api/installer.ts` — typed Tauri command/event client.
- `src/components/settings/ToolInstallDialog.tsx` — reused dialog shell for preflight, confirmation, progress, and result.
- `src/components/settings/ToolInstallDialog.test.tsx` — UI state tests.

**Modify:**

- `src-tauri/src/lib.rs` — register installer module, managed task store, and Tauri commands.
- `src-tauri/src/commands/misc.rs` — route native installs to the orchestrator while retaining update and WSL helpers.
- `src/lib/api/index.ts` — export installer API.
- `src/components/AgentLifecyclePage.tsx` — replace native install execution with orchestrated preflight/execute state.
- `src/i18n/locales/{zh,zh-TW,en,ja}.json` — install-flow labels and classified error messages.
- `src/test/setup.ts` or existing Vitest setup — only if Tauri event mocks are not already available.
- `src-tauri/Cargo.toml` — add only dependencies proven necessary by implementation.

---

### Task 1: Define Installation Domain Types and Node Policy

**Files:**
- Create: `src-tauri/src/installer/mod.rs`
- Create: `src-tauri/src/installer/model.rs`
- Create: `src-tauri/src/installer/policy.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `InstallRequest`, `EnvironmentSnapshot`, `RepairPlan`, `RepairAction`, `InstallTaskEvent`, `ToolInstallResult`, `InstallTaskResult`, `InstallFailure`, `InstallFailureCode`, `NodePolicy`, and `node_policy()`.
- Consumes: existing tool identifiers `claude`, `codex`, `gemini`, `opencode`, `openclaw`, and `hermes`.

- [ ] **Step 1: Write failing serialization and policy tests**

Add tests in `model.rs` and `policy.rs` asserting snake_case wire values and a pinned validated Node policy:

```rust
#[test]
fn task_event_serializes_with_stable_wire_names() {
    let event = InstallTaskEvent::StageChanged {
        task_id: "task-1".into(),
        stage: InstallStage::Preflight,
    };
    let value = serde_json::to_value(event).unwrap();
    assert_eq!(value["type"], "stage_changed");
    assert_eq!(value["stage"], "preflight");
}

#[test]
fn node_policy_accepts_supported_lts_and_rejects_old_node() {
    let policy = node_policy();
    assert!(policy.accepts_major(24));
    assert!(policy.accepts_major(22));
    assert!(!policy.accepts_major(18));
    assert_eq!(policy.preferred_major, 24);
}
```

- [ ] **Step 2: Run tests and verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::`

Expected: FAIL because the installer module and types do not exist.

- [ ] **Step 3: Implement serializable domain types and policy**

Use tagged enums and explicit values:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    Preflight,
    AwaitingConfirmation,
    Repairing,
    InstallingTools,
    Verifying,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InstallTaskEvent {
    StageChanged { task_id: String, stage: InstallStage },
    ActionChanged { task_id: String, action_id: String, status: ActionStatus },
    ToolFinished { task_id: String, result: ToolInstallResult },
    Finished { task_id: String, result: InstallTaskResult },
}

pub struct NodePolicy {
    pub minimum_major: u64,
    pub accepted_lts_majors: &'static [u64],
    pub preferred_major: u64,
}

pub const fn node_policy() -> NodePolicy {
    NodePolicy {
        minimum_major: 22,
        accepted_lts_majors: &[22, 24],
        preferred_major: 24,
    }
}
```

Define all spec-required task outcomes and failure codes as enums rather than strings. Add `mod installer;` to `lib.rs` without registering commands yet.

- [ ] **Step 4: Run formatter and tests**

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/installer src-tauri/src/lib.rs
git commit -m "feat: define installation task model"
```

### Task 2: Add Failure Classification and Secret Redaction

**Files:**
- Create: `src-tauri/src/installer/failure.rs`
- Modify: `src-tauri/src/installer/mod.rs`

**Interfaces:**
- Consumes: `InstallFailure` and `InstallFailureCode` from Task 1.
- Produces: `classify_process_failure(stage, exit_code, stdout, stderr) -> InstallFailure` and `redact_diagnostic(text) -> String`.

- [ ] **Step 1: Write failing classifier and redaction tests**

```rust
#[test]
fn classifies_missing_npm_without_losing_details() {
    let failure = classify_process_failure(
        InstallStage::InstallingTools,
        Some(127),
        "",
        "npm: command not found",
    );
    assert_eq!(failure.code, InstallFailureCode::DependencyMissing);
    assert_eq!(failure.recommended_action, RecommendedAction::RepairDependencies);
}

#[test]
fn redacts_tokens_proxy_passwords_and_url_credentials() {
    let raw = "ANTHROPIC_API_KEY=secret https://user:pass@example.com npm_abc123";
    let clean = redact_diagnostic(raw);
    assert!(!clean.contains("secret"));
    assert!(!clean.contains("user:pass"));
    assert!(!clean.contains("npm_abc123"));
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::failure::tests`

Expected: FAIL because classifier functions do not exist.

- [ ] **Step 3: Implement ordered classification and redaction**

Classify using typed I/O errors and exit codes first, then narrow fallback patterns for npm/system-installer output. Redact known assignments, bearer tokens, npm tokens, and URL userinfo before truncating logs:

```rust
pub fn redact_diagnostic(text: &str) -> String {
    let value = SECRET_ASSIGNMENT_RE.replace_all(text, "$1=[REDACTED]");
    let value = BEARER_RE.replace_all(&value, "Bearer [REDACTED]");
    let value = NPM_TOKEN_RE.replace_all(&value, "npm_[REDACTED]");
    URL_USERINFO_RE.replace_all(&value, "$1[REDACTED]@$2").into_owned()
}
```

Keep the raw exit code, failed stage, retryability, user-action requirement, message key, recommended action, and redacted detail in `InstallFailure`.

- [ ] **Step 4: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::failure::tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/installer/failure.rs src-tauri/src/installer/mod.rs
git commit -m "feat: classify installer failures safely"
```

### Task 3: Implement Environment Probe and Pure Repair Planner

**Files:**
- Create: `src-tauri/src/installer/probe.rs`
- Create: `src-tauri/src/installer/repair.rs`
- Modify: `src-tauri/src/installer/mod.rs`

**Interfaces:**
- Consumes: `NodePolicy`, `EnvironmentSnapshot`, `RepairAction`, and platform-neutral failure types.
- Produces: `ProbeRunner` trait, `SystemProbe::probe()`, and `build_repair_plan(snapshot, policy) -> RepairPlan`.

- [ ] **Step 1: Write failing planner tests with fake snapshots**

Cover healthy, missing, old, broken, PATH-invisible, multiple-installation, unsupported architecture, and insufficient-disk cases:

```rust
#[test]
fn missing_node_requires_confirmed_official_install() {
    let snapshot = EnvironmentSnapshot::without_node(Platform::Windows, Architecture::X64);
    let plan = build_repair_plan(&snapshot, &node_policy()).unwrap();
    assert_eq!(plan.actions.len(), 1);
    assert_eq!(plan.actions[0].kind, RepairActionKind::InstallNode);
    assert!(plan.actions[0].requires_confirmation);
    assert!(plan.actions[0].requires_elevation);
}

#[test]
fn healthy_node_and_npm_need_no_repair() {
    let snapshot = EnvironmentSnapshot::healthy_node("24.4.1", "11.4.2");
    assert!(build_repair_plan(&snapshot, &node_policy()).unwrap().actions.is_empty());
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::repair::tests`

Expected: FAIL because the planner is missing.

- [ ] **Step 3: Implement the probe abstraction and planner**

Use an injectable runner so tests never alter the host:

```rust
pub trait ProbeRunner: Send + Sync {
    fn resolve_command(&self, name: &str) -> Vec<PathBuf>;
    fn run_version(&self, path: &Path) -> CommandProbe;
    fn disk_available(&self, path: &Path) -> Result<u64, InstallFailure>;
    fn directory_writable(&self, path: &Path) -> bool;
}

pub struct SystemProbe<R: ProbeRunner> {
    runner: R,
}
```

The planner must be pure and deterministic. It blocks unsupported platforms/architectures and insufficient disk, repairs PATH before reinstalling, repairs the same Node installation when npm is broken, and never chooses/uninstalls among multiple healthy installations silently.

- [ ] **Step 4: Run probe/planner and existing Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::probe installer::repair`

Run: `cargo test --manifest-path src-tauri/Cargo.toml commands::misc::tests`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/installer/probe.rs src-tauri/src/installer/repair.rs src-tauri/src/installer/mod.rs
git commit -m "feat: probe and plan installer repairs"
```

### Task 4: Add Tool Strategies and Postflight Verification

**Files:**
- Create: `src-tauri/src/installer/tools.rs`
- Create: `src-tauri/src/installer/verifier.rs`
- Modify: `src-tauri/src/installer/mod.rs`

**Interfaces:**
- Produces: `tool_strategy(tool) -> Option<ToolInstallStrategy>` and `verify_tool(runner, strategy, before) -> ToolInstallResult`.
- Consumes: existing package names and official installer/update rules from `commands/misc.rs`.

- [ ] **Step 1: Write failing strategy and verifier tests**

```rust
#[test]
fn codex_strategy_declares_node_and_npm() {
    let strategy = tool_strategy("codex").unwrap();
    assert_eq!(strategy.command_name, "codex");
    assert_eq!(strategy.npm_package, Some("@openai/codex"));
    assert_eq!(strategy.dependencies, &[SharedDependency::Node, SharedDependency::Npm]);
}

#[test]
fn exit_zero_without_runnable_version_is_not_success() {
    let runner = FakeToolRunner::with_output(0, "", "");
    let result = verify_tool(&runner, tool_strategy("codex").unwrap(), None);
    assert_eq!(result.status, ToolInstallStatus::InstalledNotRunnable);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::tools installer::verifier`

Expected: FAIL because strategies and verifier do not exist.

- [ ] **Step 3: Implement six strategies and clean-environment verification**

Move package/command metadata, not process execution, out of `misc.rs`. Preserve official installers for Claude/OpenCode/Hermes and npm strategies for Codex/Gemini/OpenClaw. Verification resolves all candidate paths, identifies the actual default, invokes the exact resolved executable with `--version`, parses a semantic version, and reports conflicts without converting success into failure.

```rust
pub struct ToolInstallStrategy {
    pub tool: &'static str,
    pub display_name: &'static str,
    pub command_name: &'static str,
    pub npm_package: Option<&'static str>,
    pub dependencies: &'static [SharedDependency],
    pub version_args: &'static [&'static str],
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::tools installer::verifier`

Expected: PASS.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/installer/tools.rs src-tauri/src/installer/verifier.rs src-tauri/src/installer/mod.rs
git commit -m "feat: add tool install strategies and verification"
```

### Task 5: Implement Secure Windows and macOS Node Install Adapters

**Files:**
- Create: `src-tauri/src/installer/platform/mod.rs`
- Create: `src-tauri/src/installer/platform/windows.rs`
- Create: `src-tauri/src/installer/platform/macos.rs`
- Modify: `src-tauri/src/installer/mod.rs`
- Modify: `src-tauri/Cargo.toml` only if checksum or platform APIs require a dependency.

**Interfaces:**
- Produces: `PlatformAdapter` trait, `NodeRelease`, `resolve_node_release`, `download_and_verify_node`, `install_node`, and `refresh_environment`.
- Consumes: Reqwest global proxy client, Node policy, repair actions, and install failures.

- [ ] **Step 1: Write failing URL, checksum, signature-command, and authorization tests**

Use fixture release metadata and fake command runners:

```rust
#[test]
fn selects_latest_allowed_lts_for_platform_architecture() {
    let releases = fixture_releases();
    let selected = select_node_release(&releases, &node_policy(), Platform::Windows, Architecture::X64).unwrap();
    assert_eq!(selected.major, 24);
    assert!(selected.asset_name.ends_with("-x64.msi"));
}

#[test]
fn checksum_mismatch_stops_before_signature_or_install() {
    let result = verify_sha256(b"tampered", "00");
    assert_eq!(result.unwrap_err().code, InstallFailureCode::DownloadIntegrityFailure);
}
```

- [ ] **Step 2: Run platform tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::platform::`

Expected: FAIL because adapters do not exist.

- [ ] **Step 3: Implement official release selection and verified download**

Fetch `https://nodejs.org/dist/index.json`, select the newest LTS release allowed by `NodePolicy`, require the exact platform asset, download the asset and corresponding `SHASUMS256.txt`, and verify SHA-256 before platform signature checks. Downloads go into a task-owned temporary directory and are removed after completion/cancellation.

- [ ] **Step 4: Implement platform signature and privileged install behavior**

Windows adapter:

```text
powershell.exe -NoProfile -NonInteractive -Command
  (Get-AuthenticodeSignature -LiteralPath <msi>).Status
msiexec.exe /i <msi> /passive /norestart
```

Require `Valid` Authenticode status before launching `msiexec`; launch via standard elevation so UAC remains visible. macOS adapter:

```text
/usr/sbin/pkgutil --check-signature <pkg>
/usr/bin/osascript -e 'do shell script "/usr/sbin/installer -pkg ... -target /" with administrator privileges'
```

Require an Apple-recognized Developer ID Installer signature before authorization. Quote paths using argument arrays or a dedicated AppleScript string escaper. Treat declined authorization as `PrivilegeDeclined`, not `InstallerFailure`.

- [ ] **Step 5: Refresh and re-probe environment**

Windows reloads user/system PATH from the registry using existing safe helpers; macOS constructs a clean child environment from `/usr/libexec/path_helper -s` plus the login shell. Neither adapter mutates the parent OS environment beyond the approved installer/PATH actions.

- [ ] **Step 6: Run platform tests and cross-platform compile checks**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::platform::`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS on the current platform; platform-specific modules remain cfg-gated and test their pure command builders on all hosts.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/installer/platform src-tauri/src/installer/mod.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: install verified Node runtime on desktop"
```

### Task 6: Build the Orchestrator, Task Store, and Tauri Commands

**Files:**
- Create: `src-tauri/src/installer/orchestrator.rs`
- Modify: `src-tauri/src/installer/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/misc.rs`

**Interfaces:**
- Produces Tauri commands:
  - `prepare_tool_install(request: InstallRequest) -> InstallPreparation`
  - `start_tool_install(request: ConfirmedInstallRequest) -> String`
  - `get_install_task(task_id: String) -> InstallTaskSnapshot`
  - `cancel_install_task(task_id: String) -> Result<(), String>`
- Emits `agent-manager://install-task` payloads of type `InstallTaskEvent`.

- [ ] **Step 1: Write failing orchestrator tests with fake adapters**

```rust
#[tokio::test]
async fn batch_repairs_dependencies_once_and_isolates_tool_failures() {
    let harness = Harness::missing_node()
        .tool_result("codex", ToolInstallStatus::Succeeded)
        .tool_result("gemini", ToolInstallStatus::Failed);
    let result = harness.run_confirmed(&["codex", "gemini"]).await;
    assert_eq!(harness.node_install_count(), 1);
    assert_eq!(result.tools.len(), 2);
    assert_eq!(result.status, InstallTaskStatus::NeedsUserAction);
}

#[tokio::test]
async fn unconfirmed_privileged_plan_cannot_execute() {
    let error = Harness::missing_node().run_without_confirmation(&["codex"]).await.unwrap_err();
    assert_eq!(error.code, InstallFailureCode::PrivilegeDeclined);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::orchestrator::tests`

Expected: FAIL because the orchestrator is missing.

- [ ] **Step 3: Implement deterministic task transitions**

Only allow:

```text
created -> preflight -> awaiting_confirmation -> repairing
created -> preflight -> installing_tools
repairing -> installing_tools -> verifying -> completed
any active non-system-installer stage -> cancelled
```

Store tasks in `Arc<RwLock<HashMap<String, InstallTaskSnapshot>>>`, use an atomic cancellation token checked between actions, emit every transition, and persist enough task metadata under the app config directory to re-probe interrupted work on startup. Never resume an old privileged command blindly.

- [ ] **Step 4: Implement batch isolation and legacy routing**

Run shared dependency repair once, then process each requested tool independently. Preserve the existing WSL and update route in `run_tool_lifecycle_action`; native `install` calls the new orchestrator compatibility entry until the frontend migrates.

- [ ] **Step 5: Register managed state and Tauri commands**

In `lib.rs`, initialize the task store in `.manage(...)`, register the four commands, and pass `AppHandle` to emit task events. Ensure close handling can query active tasks before final exit behavior is changed in Task 8.

- [ ] **Step 6: Run Rust tests and check**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installer::orchestrator::tests`

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Run: `cargo check --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add src-tauri/src/installer src-tauri/src/lib.rs src-tauri/src/commands/misc.rs
git commit -m "feat: orchestrate verified tool installation"
```

### Task 7: Add Typed Frontend API and Reused Installation Dialog

**Files:**
- Create: `src/lib/api/installer.ts`
- Modify: `src/lib/api/index.ts`
- Create: `src/components/settings/ToolInstallDialog.tsx`
- Create: `src/components/settings/ToolInstallDialog.test.tsx`
- Modify: `src/i18n/locales/zh.json`
- Modify: `src/i18n/locales/zh-TW.json`
- Modify: `src/i18n/locales/en.json`
- Modify: `src/i18n/locales/ja.json`

**Interfaces:**
- Consumes: Tauri commands/events from Task 6.
- Produces: `installerApi.prepare`, `installerApi.start`, `installerApi.getTask`, `installerApi.cancel`, `installerApi.listen`, and `ToolInstallDialog`.

- [ ] **Step 1: Write failing dialog tests**

Use Testing Library to verify existing dialog primitives, confirmation gating, progress, partial results, and redacted diagnostics:

```tsx
it("lists privileged repair actions before enabling confirmation", async () => {
  render(<ToolInstallDialog state={missingNodePreparation} {...handlers} />);
  expect(screen.getByText(/Node.js 24 LTS/i)).toBeInTheDocument();
  expect(screen.getByText(/administrator/i)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("checkbox"));
  expect(screen.getByRole("button", { name: /continue/i })).toBeEnabled();
});

it("shows successful and failed tools in one batch result", () => {
  render(<ToolInstallDialog state={partialBatchResult} {...handlers} />);
  expect(screen.getByText("Codex")).toHaveTextContent(/installed/i);
  expect(screen.getByText("Gemini CLI")).toHaveTextContent(/failed/i);
});
```

- [ ] **Step 2: Run tests and verify failure**

Run: `pnpm test:unit -- ToolInstallDialog.test.tsx`

Expected: FAIL because the API and component do not exist.

- [ ] **Step 3: Implement typed API and event subscription**

Mirror Rust snake_case wire types exactly. `listen` must return Tauri's unlisten callback and filter by `task_id` before updating React state.

```ts
export const installerApi = {
  prepare: (request: InstallRequest) =>
    invoke<InstallPreparation>("prepare_tool_install", { request }),
  start: (request: ConfirmedInstallRequest) =>
    invoke<string>("start_tool_install", { request }),
  getTask: (taskId: string) =>
    invoke<InstallTaskSnapshot>("get_install_task", { taskId }),
  cancel: (taskId: string) =>
    invoke<void>("cancel_install_task", { taskId }),
};
```

- [ ] **Step 4: Implement the dialog by composing existing UI**

Use `Dialog`, `DialogContent`, `DialogHeader`, `DialogFooter`, `Button`, existing status colors/icons, and `ToolInstallRow` where paths are shown. Render four modes in one component: preflight, authorization, progress, and result. Do not duplicate the tool cards or create a second lifecycle page.

- [ ] **Step 5: Add complete localized copy**

Add matching keys to all four locale files for stages, repair actions, authorization, result statuses, failure codes, retry/cancel, and diagnostics. English/Chinese/Japanese objects must have identical key sets.

- [ ] **Step 6: Run frontend checks**

Run: `pnpm test:unit -- ToolInstallDialog.test.tsx`

Run: `pnpm typecheck`

Run: `pnpm format:check`

Expected: PASS.

- [ ] **Step 7: Commit**

```powershell
git add src/lib/api src/components/settings/ToolInstallDialog.tsx src/components/settings/ToolInstallDialog.test.tsx src/i18n/locales
git commit -m "feat: add installation repair dialog"
```

### Task 8: Integrate the Orchestrator into AgentLifecyclePage

**Files:**
- Modify: `src/components/AgentLifecyclePage.tsx`
- Modify: `src-tauri/src/lib.rs`
- Test: `src/components/settings/ToolInstallDialog.test.tsx`

**Interfaces:**
- Consumes: `installerApi` and `ToolInstallDialog` from Task 7.
- Preserves: existing update confirmation, WSL shell selection, tool cards, conflict diagnostics, version refresh, batch isolation, and busy locking.

- [ ] **Step 1: Add a failing integration-oriented UI test**

Mock `prepare_tool_install`, `start_tool_install`, and the event listener. Assert a native install opens preflight, a healthy preparation starts without privilege confirmation, a privileged plan waits for confirmation, and final events refresh only completed tool cards.

- [ ] **Step 2: Run the test and verify failure**

Run: `pnpm test:unit -- ToolInstallDialog.test.tsx`

Expected: FAIL because lifecycle integration is not wired.

- [ ] **Step 3: Replace only the native install branch**

In `handleRunToolAction`:

```ts
if (action === "install" && selectedToolsAreNative(toolNames)) {
  const preparation = await installerApi.prepare({ tools: toolNames, action });
  setInstallFlow({ mode: "prepared", preparation });
  return;
}
```

Continue using the current `executeRun` path for update and WSL. Keep `preflightTools`/`isAnyBusy` as the single UI lock, and map backend events into the dialog instead of duplicating action state per component.

- [ ] **Step 4: Handle close, cancel, completion, and refresh**

Closing the dialog hides it without cancelling. The explicit cancel button calls the backend. On `finished`, refresh each reported tool, refresh conflict diagnostics for installed-but-broken/conflict outcomes, and reuse the existing toast summary only as a compact notification.

Before app exit, if the backend reports an active task, show the existing dialog system with a clear warning; do not force-close a running MSI/PKG. Preserve current immediate exit when there is no active task.

- [ ] **Step 5: Run frontend and Rust regression checks**

Run: `pnpm test:unit`

Run: `pnpm typecheck`

Run: `pnpm build:renderer`

Run: `cargo test --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

- [ ] **Step 6: Commit**

```powershell
git add src/components/AgentLifecyclePage.tsx src/components/settings/ToolInstallDialog.test.tsx src-tauri/src/lib.rs
git commit -m "feat: integrate guided installation flow"
```

### Task 9: Full Verification and Platform Evidence

**Files:**
- Create: `docs/superpowers/verification/2026-07-13-installation-orchestrator.md`
- Modify implementation files only for defects exposed by verification.

**Interfaces:**
- Consumes all previous tasks.
- Produces an evidence record with exact commands, platform limitations, and remaining risks.

- [ ] **Step 1: Run the complete local quality gate**

```powershell
pnpm format:check
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: every command exits 0.

- [ ] **Step 2: Run safe fake-installer integration scenarios**

Exercise fixture-backed missing npm, missing Node, old Node, PATH refresh, checksum mismatch, signature rejection, privilege decline, proxy timeout, partial batch failure, installed-not-runnable, cancellation, and interrupted-task re-probe. Tests must use temporary directories and fake command runners.

- [ ] **Step 3: Verify Windows in a disposable VM**

Record OS build and architecture. Test clean install, UAC accept/decline, PATH refresh, multiple Node installations, proxy failure, file lock, disk-space guard, batch partial failure, and postflight path/version. Do not use the developer workstation as a clean-install target.

- [ ] **Step 4: Verify macOS on disposable Intel and Apple Silicon environments**

Record macOS version and architecture. Test PKG authorization accept/decline, signature rejection, GUI/zsh PATH difference, `/usr/local` versus `/opt/homebrew`, root-owned npm prefix, proxy/TLS failure, batch partial failure, and postflight path/version. If either hardware class is unavailable, mark that exact scenario blocked rather than inferring success.

- [ ] **Step 5: Write verification evidence**

Document every command and result, fixture coverage, VM identifiers, versions, screenshots/log locations, known limitations, and any unexecuted platform case. Do not claim macOS or Windows evidence that was not actually collected.

- [ ] **Step 6: Final commit**

```powershell
git add docs/superpowers/verification/2026-07-13-installation-orchestrator.md
git commit -m "docs: verify installation orchestrator"
```

