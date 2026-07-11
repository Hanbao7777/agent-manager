# Agent Manager Trimming Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Trim and rebrand the CC Switch 3.16.5 desktop application into a single-page Agent Manager while preserving its existing six-agent detection, installation, update, and diagnostics behavior.

**Architecture:** Keep the existing lifecycle implementation in `src-tauri/src/commands/misc.rs` and its `settingsApi` bindings as the only source of agent state and command generation. First shut down upstream application-update behavior, then extract the lifecycle portion of `AboutSection` into the sole product surface, disconnect excluded frontend and startup paths, and delete only code proven unreachable by references and compile gates. Finish with metadata, dependency, resource, and residual scans without introducing replacement lifecycle logic.

**Tech Stack:** React 18, TypeScript, Vite, Vitest, pnpm, Tauri 2, Rust 1.85+, Cargo, React Query, i18next, existing CC Switch lifecycle commands and shell installers.

---

## Global Constraints

The following constraints are copied from the approved design and apply to every task:

- 以删除或断开功能为主，不以重写替代现有实现。
- 现有生命周期行为没有已确认缺陷时，不改变其命令、数据流和错误语义。
- 目标平台为 Windows 和 macOS；保留生命周期代码现有的 WSL 支持。
- 产品名称统一为 `Agent Manager`。
- Agent Manager 不得继续访问 CC Switch 官方自动更新地址。
- 保留 MIT License 与上游归属信息，不把归属声明误删为品牌残留。
- 现有未提交删除必须保持原状，不得在裁剪中意外恢复或混入无关提交。
- Do not add a second command generator, agent manifest, or installation-state source; use the existing Tauri lifecycle API path.
- Do not modify PATH automatically or remove duplicate installations automatically.

The repository currently has a large unstaged deletion set inherited from the upstream documentation trim. Before every commit, use `git status --short` and stage only the task files listed for that task; never restore, stage, or commit the existing deletions.

## File Map and Retention Boundaries

- `src/components/settings/AboutSection.tsx`: split the retained lifecycle UI from the removed application self-update UI. Preserve `TOOL_NAMES`, `getToolVersions`, `runToolLifecycleAction`, `probeToolInstallations`, shell selection, confirmation, diagnostics, and error feedback.
- `src/components/AgentLifecyclePage.tsx`: new focused page component containing the extracted lifecycle JSX and state/handlers from `AboutSection`, without updater controls.
- `src/App.tsx`, `src/main.tsx`: replace the provider/settings shell with the lifecycle page and retain only providers needed for theme, query, i18n, dialog/process error handling, and notifications.
- `src/lib/api/settings.ts`, `src-tauri/src/commands/misc.rs`: retained lifecycle interfaces. Do not change command names, installer definitions, output/error handling, or supported tool list except to remove unrelated adjacent APIs after references are gone.
- `src-tauri/src/lib.rs`, `src-tauri/src/commands/mod.rs`: startup and command registration pruning. Keep lifecycle commands and their actual runtime dependencies; remove excluded commands and startup tasks.
- `src-tauri/src/commands/settings.rs`, `src/components/DatabaseUpgrade.tsx`, `src/contexts/UpdateContext.tsx`, `src/lib/updater.ts`, `src/components/UpdateBadge.tsx`: remove self-update behavior after the updater safety shutdown is in place.
- `src-tauri/tauri.conf.json`, `package.json`, `src-tauri/Cargo.toml`, lockfiles, icons, localization, and app metadata: final branding and resource/dependency cleanup.

## Task 1: Establish a Clean Baseline and Lifecycle Characterization Tests

**Files:**
- Create: `tests/components/AgentLifecyclePage.characterization.test.tsx` only if existing test setup can render the extracted lifecycle component; otherwise add the smallest focused test beside the component.
- Create/Modify: `src/components/settings/AboutSection.tsx` only if a test seam/export is required; do not refactor behavior yet.
- Inspect only: `src/lib/api/settings.ts`, `src-tauri/src/commands/misc.rs`, existing `tests/components/*` and `tests/utils/*`.

- [ ] **Step 1: Record the baseline and verify the committed design.**

Run:

```bash
git status --short --branch
git diff --exit-code -- docs/superpowers/specs/2026-07-11-agent-manager-trimming-design.md
```

Expected: the existing upstream documentation deletions remain unstaged; the design file matches commit `fa68eab`; no application file is changed.

- [ ] **Step 2: Inspect the retained interfaces before writing tests.**

Confirm these signatures and constants are unchanged:

```text
settingsApi.getToolVersions(tools?, wslShellByTool?)
settingsApi.runToolLifecycleAction(tools, action, wslShellByTool?)
settingsApi.probeToolInstallations(tools)
VALID_TOOLS = claude, codex, gemini, opencode, openclaw, hermes
```

Expected: the frontend calls those three APIs and the Rust commands remain registered from `src-tauri/src/lib.rs`.

- [ ] **Step 3: Add characterization tests for the lifecycle UI contract.**

Test the extracted/future component’s observable contract with mocked `settingsApi` calls:

```text
render shows all six tool names;
initial load calls getToolVersions once per tool or through the existing equivalent flow;
install/update invokes runToolLifecycleAction with the selected tool and action;
diagnose invokes probeToolInstallations and renders a conflict result;
failed lifecycle calls produce the existing error toast path;
the batch path continues to process independent tool results according to existing behavior.
```

Do not assert implementation details such as hook order, CSS, or a new command shape.

- [ ] **Step 4: Run the focused characterization tests.**

Run:

```bash
pnpm test:unit -- tests/components/AgentLifecyclePage.characterization.test.tsx
```

Expected: tests pass against the current behavior, or any pre-existing test-environment failure is recorded with its exact output before proceeding.

- [ ] **Step 5: Commit only the characterization test if it is independently useful.**

```bash
git add tests/components/AgentLifecyclePage.characterization.test.tsx
git commit -m "test: characterize retained agent lifecycle UI"
```

If no new test file is needed because existing tests already cover every listed behavior, do not create an empty commit; record the evidence in the implementation PR/task notes instead.

## Task 2: Disable the Upstream Update Path Before Producing a Runnable Build

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src/components/settings/AboutSection.tsx`
- Modify/Delete: `src/components/DatabaseUpgrade.tsx`, `src/contexts/UpdateContext.tsx`, `src/components/UpdateBadge.tsx`, `src/lib/updater.ts`, and direct tests only after reference search.
- Modify: `src/main.tsx`, `src/lib/api/settings.ts` as required to remove updater calls without changing retained lifecycle APIs.
- Modify: `package.json`, `src-tauri/Cargo.toml`, lockfiles only after imports and plugin initialization are gone.

- [ ] **Step 1: Add a static safety test/script for updater residue.**

Create a repository-local test or documented shell assertion that fails when application source/config contains either the upstream endpoint or updater public key:

```bash
! git grep -n -E 'farion1231/cc-switch/releases|latest\.json|dW50cnVzdGVkIGNvbW1lbnQ6' -- ':!docs/superpowers/specs/*' ':!docs/superpowers/plans/*'
```

Expected before implementation: FAIL because the current updater configuration is present. Keep the check focused on product source/config, not preserved MIT attribution.

- [ ] **Step 2: Remove the updater endpoint, updater public key, and updater artifact generation.**

In `src-tauri/tauri.conf.json`, remove the `plugins.updater` object and set `bundle.createUpdaterArtifacts` to false or remove it according to the Tauri schema. Do not remove unrelated bundle targets or icons in this task.

- [ ] **Step 3: Remove frontend update entry points and state.**

Remove the `UpdateProvider` wrapper and `UpdateContext` usage from `src/main.tsx`; remove the update badge and update controls from `App.tsx`; remove only the application-update section of `AboutSection` while retaining the lifecycle section. Delete `DatabaseUpgrade` rendering and its `install_update_and_restart` invocation rather than leaving a dead “upgrade application” path.

- [ ] **Step 4: Remove backend update commands and plugin initialization.**

Remove `install_update_and_restart`, `check_app_update_available`, and `check_for_updates` from command registration and delete their implementations only after `git grep` confirms no retained caller. Remove `tauri-plugin-updater` initialization and the Cargo dependency. Keep `tauri-plugin-process` if still needed for exit/restart or retained error handling.

- [ ] **Step 5: Run compile and residue checks before continuing.**

Run:

```bash
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo check --manifest-path src-tauri/Cargo.toml
git grep -n -E 'farion1231/cc-switch/releases|latest\.json|tauri-plugin-updater|install_update_and_restart|check_app_update_available|check_for_updates' -- ':!docs/superpowers/specs/*' ':!docs/superpowers/plans/*' || true
```

Expected: all available compile/test gates pass; the final grep returns no product updater endpoint, plugin, command, or update UI references. If a config-load recovery path still requires an update command, remove that recovery UI and preserve only the normal configuration error/exit behavior.

- [ ] **Step 6: Commit the safety shutdown.**

```bash
git add src-tauri/tauri.conf.json src-tauri/src/lib.rs src-tauri/src/commands/settings.rs src/main.tsx src/App.tsx src/components/settings/AboutSection.tsx src/components/DatabaseUpgrade.tsx src/contexts/UpdateContext.tsx src/components/UpdateBadge.tsx src/lib/updater.ts src/lib/api/settings.ts package.json src-tauri/Cargo.toml pnpm-lock.yaml src-tauri/Cargo.lock
git commit -m "chore: disable upstream CC Switch updater"
```

Stage only files that actually changed; do not stage existing deleted docs.

## Task 3: Extract the Lifecycle UI Without Changing Behavior

**Files:**
- Create: `src/components/AgentLifecyclePage.tsx`
- Modify: `src/components/settings/AboutSection.tsx`
- Modify: `src/components/settings/ToolInstallRow.tsx`, `src/components/settings/ToolUpgradeConfirmDialog.tsx` only if props currently depend on settings-only state.
- Test: `tests/components/AgentLifecyclePage.characterization.test.tsx` or the existing lifecycle component test.

- [ ] **Step 1: Define the extracted component boundary.**

Move the lifecycle-specific state, handlers, constants, and JSX from `AboutSection` into `AgentLifecyclePage`. Preserve these existing calls and data flow exactly:

```ts
settingsApi.getToolVersions(toolNames, wslShellByTool)
settingsApi.runToolLifecycleAction([toolName], action, wslShellByTool)
settingsApi.probeToolInstallations(toolNames)
```

Keep the existing `TOOL_NAMES`, tool display names, WSL shell options, `executeRun`, preflight lock, confirmation dialog, diagnostics, refresh/cache behavior, stdout/stderr-derived error display, and batch partial-failure handling. Do not retain `getVersion`, `useUpdate`, release-note links, `isPortable`, or updater-only icons/state.

- [ ] **Step 2: Make the old AboutSection unavailable to the product surface.**

Replace the lifecycle JSX in `AboutSection` with the new component only if the settings page remains temporarily reachable for a test transition. The final single-page shell must import `AgentLifecyclePage` directly; `AboutSection` must not be the source of lifecycle behavior in two places.

- [ ] **Step 3: Run focused tests and static API checks.**

Run:

```bash
pnpm test:unit -- tests/components/AgentLifecyclePage.characterization.test.tsx
pnpm typecheck
```

Expected: lifecycle tests pass and `git grep` shows only the extracted component calling the three retained lifecycle APIs from the frontend.

- [ ] **Step 4: Commit the extraction.**

```bash
git add src/components/AgentLifecyclePage.tsx src/components/settings/AboutSection.tsx src/components/settings/ToolInstallRow.tsx src/components/settings/ToolUpgradeConfirmDialog.tsx tests/components/AgentLifecyclePage.characterization.test.tsx
git commit -m "refactor: extract agent lifecycle page"
```

## Task 4: Establish the Single-Page Shell

**Files:**
- Modify: `src/App.tsx`
- Modify: `src/main.tsx`
- Modify: `src/index.css` only for lifecycle-page layout needs
- Modify: `src/components/theme-provider.tsx` or shared UI only if required by existing theme behavior
- Test: `tests/components/AgentLifecyclePage.characterization.test.tsx` and a new `tests/components/App.single-page.test.tsx` if current App tests do not cover routing.

- [ ] **Step 1: Add a failing shell test.**

Assert that the root render exposes the lifecycle page and does not expose excluded navigation labels or controls:

```text
Agent Manager renders the six-agent lifecycle surface;
provider, proxy, MCP, skills, prompts, sessions, workspace, and usage navigation are absent;
there is no settings/about route required to reach lifecycle actions.
```

- [ ] **Step 2: Replace App’s provider/navigation surface.**

Remove the active-app/provider state, provider queries/actions, view union, provider switch listeners, proxy status, usage cache bridge, environment-conflict startup checks, profile/app switcher, and excluded panel rendering from `src/App.tsx`. Render `<AgentLifecyclePage />` as the main content while retaining only window behavior, theme-compatible layout, and toast/error plumbing needed by the lifecycle page.

- [ ] **Step 3: Keep only required bootstrap providers.**

In `src/main.tsx`, retain `QueryClientProvider` only if `AgentLifecyclePage` still uses existing query infrastructure, retain `ThemeProvider`, i18n, and `Toaster`, and remove `UpdateProvider`, `DatabaseUpgrade`, update listeners, and unrelated imports. Do not replace Tauri lifecycle APIs with a new frontend service.

- [ ] **Step 4: Run shell gates.**

```bash
pnpm test:unit -- tests/components/App.single-page.test.tsx tests/components/AgentLifecyclePage.characterization.test.tsx
pnpm typecheck
pnpm build:renderer
```

Expected: one main page loads, lifecycle controls remain reachable, and no excluded UI is rendered.

- [ ] **Step 5: Commit the single-page shell.**

```bash
git add src/App.tsx src/main.tsx src/index.css src/components/theme-provider.tsx tests/components/App.single-page.test.tsx tests/components/AgentLifecyclePage.characterization.test.tsx
git commit -m "feat: make agent lifecycle the single product surface"
```

## Task 5: Apply Agent Manager Metadata and Branding

**Files:**
- Modify: `package.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src/index.html`, `src/components/AgentLifecyclePage.tsx`, retained translations under `src/i18n/`
- Modify: `src/assets/icons/*`, `src-tauri/icons/*`, bundle templates/manifests where the current product name is user-visible
- Preserve: `LICENSE` and explicit upstream attribution/source notice.

- [ ] **Step 1: Add a metadata residue scan before changing values.**

Run:

```bash
```

Expected: output identifies every product-visible or configuration identity that must be classified as either removable branding or preserved attribution.

- [ ] **Step 2: Rename product metadata without changing lifecycle version semantics.**

Set product/package descriptions and visible names to `Agent Manager`. Choose a new stable bundle identifier and protocol-free configuration identity only after confirming no retained code uses the old identifier for lifecycle paths. Keep versioning policy explicit: do not claim an upstream CC Switch release identity for a branded Agent Manager release unless the controller chooses that policy.

- [ ] **Step 3: Replace visible branding and resources.**

Update window title, HTML title, lifecycle-page heading, alt text, icons, bundle display metadata, and installer metadata. Remove CC Switch product wording from UI and release-facing metadata, but retain `LICENSE` and a concise upstream attribution/source notice.

- [ ] **Step 4: Run metadata tests/scans.**

```bash
pnpm typecheck
pnpm build:renderer
```

Expected: no product-visible CC Switch identity or old bundle identifier remains; any retained attribution is outside the scanned product metadata or explicitly documented.

- [ ] **Step 5: Commit branding.**

```bash
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src/index.html src src-tauri/icons src-tauri/*.yml src-tauri/*.xml
git commit -m "rebrand: rename product to Agent Manager"
```

## Task 6: Disconnect and Delete Excluded Frontend Feature Domains

**Files:**
- Modify: `src/App.tsx` and retained page shell files
- Delete only after reference checks: provider components/hooks/API, proxy/routing/failover components/hooks/API, usage/session/workspace components/hooks/API, MCP/skills/prompts/profile/import-export/sync components/hooks/API, OpenClaw/Hermes configuration panels/hooks
- Modify/Delete: `src/types.ts` and feature-specific type files after all imports are removed
- Modify: `src/i18n/*` to remove only unreachable keys/locales while retaining lifecycle, theme, shell, and error strings.

- [ ] **Step 1: Generate a feature-domain reference inventory.**

For each excluded domain, run:

```bash
```

Classify each hit as reachable from `AgentLifecyclePage`, required shared infrastructure, or delete candidate. Do not delete a file solely because its directory name is excluded.

- [ ] **Step 2: Add a reachable-surface test.**

Extend `tests/components/App.single-page.test.tsx` to assert excluded navigation and feature components are not mounted. Mock only retained Tauri lifecycle calls; do not create mocks for deleted commands.

- [ ] **Step 3: Delete frontend entry points first.**

Remove excluded imports, state, effects, event listeners, toolbar buttons, dialogs, and navigation from `src/App.tsx`. Remove the corresponding API wrappers and hooks only after `git grep` reports no imports. Keep shared `Button`, dialog, toast, theme, platform, error, and i18n utilities when used by lifecycle UI.

- [ ] **Step 4: Delete unreachable frontend domains and translations.**

Remove component/domain files proven unreachable. Remove only translation namespaces/keys no longer referenced by retained code; preserve tool names, environment labels, install/update/diagnostic errors, common UI, theme, and shell strings.

- [ ] **Step 5: Run frontend gates and import scans.**

```bash
pnpm typecheck
pnpm test:unit
pnpm build:renderer
```

Expected: compile/tests/build pass; no excluded component is imported from the product entry; retained lifecycle UI remains the only reachable feature surface.

- [ ] **Step 6: Commit frontend deletion batch.**

```bash
git add src tests
git commit -m "refactor: remove excluded frontend features"
```

## Task 7: Map OpenClaw/Hermes Shared Boundaries Before Backend Deletion

**Files:**
- Inspect/modify: `src-tauri/src/commands/misc.rs`
- Inspect: `src-tauri/src/openclaw_config.rs`, `src-tauri/src/hermes_config.rs`, `src-tauri/src/commands/openclaw.rs`, `src-tauri/src/commands/hermes.rs`
- Modify: `src/lib/api/settings.ts` and lifecycle types only if unrelated OpenClaw/Hermes API exports keep excluded imports alive.
- Create: `docs/superpowers/plans/2026-07-11-agent-manager-openclaw-hermes-boundary.md` only if the implementer needs a committed mapping artifact; otherwise keep the mapping in the task review.

- [ ] **Step 1: Record the retained/shared/excluded matrix.**

The matrix must explicitly identify:

```text
Retained: misc.rs VALID_TOOLS, get_tool_versions, run_tool_lifecycle_action,
          probe_tool_installations, installer command definitions, WSL detection.
Excluded: OpenClaw/Hermes provider/config panels, health/config/default-model/env/tools/memory
          commands, hooks, API clients, and non-installation schemas.
Shared: tool names/display labels, environment/path helpers, generic errors, and any
        serialization types directly required by retained probe/install code.
```

- [ ] **Step 2: Add/retain characterization tests for all six tools.**

Ensure Rust tests cover command planning/normalization and frontend tests cover each tool card. Do not change installer URLs, shell command construction, or error parsing merely to make deletion easier.

- [ ] **Step 3: Remove only non-lifecycle OpenClaw/Hermes callers.**

Delete frontend and Tauri command registrations for configuration management after `git grep` confirms they are not used by the lifecycle APIs. Keep any shared helper required by `misc.rs`; if a helper is shared, leave it in place rather than reimplementing a narrower copy.

- [ ] **Step 4: Run Rust and frontend gates.**

```bash
pnpm typecheck
pnpm test:unit
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: six-tool lifecycle command planning still compiles/tests; no OpenClaw/Hermes configuration command is registered.

- [ ] **Step 5: Commit the shared-boundary deletion.**

```bash
git add src src-tauri/src tests
git commit -m "refactor: keep only OpenClaw and Hermes lifecycle support"
```

## Task 8: Prune Backend Startup Side Effects and Commands

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify/Delete after reference inventory: `src-tauri/src/store.rs`, `database/`, `services/`, `tray.rs`, `usage_events.rs`, `session_manager/`, proxy modules, provider/config modules, sync modules, profile/import/export modules, and associated tests.
- Modify: `src-tauri/src/settings.rs` to retain only settings required by lifecycle detection/install/shell behavior.

- [ ] **Step 1: Characterize startup behavior before pruning.**

Run the current application in a test environment and capture startup logs, then inspect `src-tauri/src/lib.rs` for:

```text
Database::init and migrations;
AppState::new and managed state;
proxy restore/takeover;
usage backfill/cache worker;
WebDAV/S3 workers;
deep-link registration;
tray menu construction and provider/usage refresh;
skill migration and other provider migrations.
```

Expected: a written retained/deleted startup table showing exactly which initialization is needed by `AgentLifecyclePage` and which is excluded side effect.

- [ ] **Step 2: Replace the startup composition with the minimum retained composition.**

Remove excluded workers, migrations, proxy restore, usage refresh, provider/tray synchronization, and state construction only after confirming lifecycle commands do not take `State<AppState>` or database-backed provider services. Keep logging, dialog/process plugins, window state if needed for the shell, and the lifecycle command registration.

- [ ] **Step 3: Prune command registration by reachable interface.**

In `commands/mod.rs` and the `invoke_handler` list in `src-tauri/src/lib.rs`, retain only lifecycle commands plus generic commands actually called by the single page. Remove provider/proxy/usage/MCP/skills/prompts/sessions/workspace/profile/sync/deeplink commands. Verify no frontend `invoke` or API wrapper still names a removed command.

- [ ] **Step 4: Remove backend modules and dependencies made unreachable.**

Delete modules only after `cargo check` identifies no imports. Preserve `misc.rs` lifecycle support and shared platform/shell helpers. Do not remove `dirs`, `regex`, `reqwest`, shell/process, serialization, or platform dependencies until the retained code no longer uses them.

- [ ] **Step 5: Run backend gates and startup residue scans.**

```bash
cargo fmt --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
git grep -n -E 'start_worker|restore_proxy_state_on_startup|UsageCache|ProxyService|webdav|s3_sync|queryProviderUsage|migrate_skill|list_profiles' -- src-tauri/src || true
```

Expected: Rust checks pass; excluded workers/services are absent from startup and no deleted command is registered or called.

- [ ] **Step 6: Commit backend startup and command pruning.**

```bash
git add src-tauri/src
git commit -m "refactor: prune excluded backend startup and commands"
```

## Task 9: Remove Deep Link as a Complete Feature Domain

**Files:**
- Modify/Delete: `src-tauri/src/deeplink/`, `src-tauri/src/commands/deeplink.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Modify: frontend API/types/tests only if any deep-link import remains after Task 6.

- [ ] **Step 1: Add a failing residue check.**

```bash
```

Expected before deletion: matches in Rust handlers, plugin setup, Tauri scheme configuration, and possibly frontend imports.

- [ ] **Step 2: Remove frontend deep-link entry points and commands.**

Delete deep-link dialogs/API wrappers and remove `parse_deeplink`, `merge_deeplink_config`, `import_from_deeplink`, and unified variants from command registration after caller search.

- [ ] **Step 3: Remove Rust plugin setup and handlers.**

Remove `DeepLinkExt`, plugin initialization, URL registration, `handle_deeplink_url`, and startup event handlers from `src-tauri/src/lib.rs`. Delete the deeplink module and command module only after compile references are gone.

- [ ] **Step 4: Remove dependency and Tauri scheme configuration.**

Remove `tauri-plugin-deep-link` from Cargo manifests/lockfile and remove `plugins.deep-link` from `tauri.conf.json`. Do not replace it with another protocol or scheme.

- [ ] **Step 5: Verify complete removal.**

```bash
cargo check --manifest-path src-tauri/Cargo.toml
pnpm typecheck
pnpm test:unit
```

Expected: checks pass and the grep returns no source/config residue.

- [ ] **Step 6: Commit deep-link removal.**

```bash
git add src src-tauri package.json pnpm-lock.yaml
git commit -m "refactor: remove CC Switch deep link integration"
```

## Task 10: Clean Dependencies, i18n, Resources, and Build Configuration

**Files:**
- Modify: `package.json`, `pnpm-lock.yaml`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`
- Modify/Delete: `src/i18n/*`, unused assets/icons, bundle manifests/templates, `flatpak/` metadata if still tracked and relevant
- Modify: `src-tauri/tauri.conf.json`, `src-tauri/build.rs` if present, and platform bundle configuration.

- [ ] **Step 1: Build a dependency usage list.**

For every candidate dependency, confirm usage with:

```bash
git grep -n 'from "<package>"\|require("<package>")' -- src tests
git grep -n '<crate-name>' -- src-tauri/src src-tauri/Cargo.toml
```

Remove only packages/crates with zero retained references. Do not remove Tauri dialog/process/store or generic serialization/network/process crates until retained lifecycle code no longer uses them.

- [ ] **Step 2: Remove unreachable translations and resources.**

Use the retained component imports and translation keys as the allowlist. Delete unused provider/proxy/usage/session/MCP/skills/prompts/sync/deep-link strings and assets; preserve lifecycle labels/errors, theme/common strings, MIT license, and upstream attribution.

- [ ] **Step 3: Update lockfiles and validate package manifests.**

Run:

```bash
pnpm install --lockfile-only
cargo check --manifest-path src-tauri/Cargo.toml
pnpm typecheck
```

Expected: lockfiles reflect only declared dependencies and both package ecosystems resolve successfully.

- [ ] **Step 4: Commit cleanup.**

```bash
git add package.json pnpm-lock.yaml src-tauri/Cargo.toml src-tauri/Cargo.lock src/i18n src/assets src-tauri/icons src-tauri/tauri.conf.json flatpak
git commit -m "chore: clean Agent Manager dependencies and resources"
```

## Task 11: Cross-Platform Verification and Final Residual Scans

**Files:**
- Modify only if verification finds a concrete regression: the smallest affected source/config file.
- Create: `docs/superpowers/verification/2026-07-11-agent-manager-trimming.md` with command output, platform/build evidence, and scan results.

- [ ] **Step 1: Run the required repository gates.**

```bash
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo fmt --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands pass. Record exact failures instead of weakening tests or changing lifecycle behavior to force a pass.

- [ ] **Step 2: Run lifecycle smoke checks on Windows and macOS.**

For each platform, record:

```text
application starts directly on the lifecycle page;
all six tools show installed/not-installed/broken status appropriately;
one missing tool installs successfully or reports the official command error;
one installed tool upgrades or reports no update;
batch install continues and reports independent failures;
multi-install/PATH conflict diagnostics display the existing report/confirmation;
no application updater request is made.
```

Use the existing official agent distribution channels and a disposable test profile; do not alter user PATH or remove duplicate installations.

- [ ] **Step 3: Verify WSL and shell behavior on Windows.**

Exercise the existing `wslShellByTool` selection through at least one WSL tool and record that shell selection, `--version` detection, install/update command execution, and error propagation remain unchanged.

- [ ] **Step 4: Run final source/config residue scans.**

```bash
git grep -n -E 'start_worker|UsageCache|ProxyService|webdav|s3_sync|register_all|on_open_url|install_update_and_restart|check_app_update_available' -- src src-tauri || true
```

Expected: only explicitly preserved upstream attribution/license or legitimate retained lifecycle terminology remains; no excluded command, plugin, worker, updater endpoint, old identity, or protocol is reachable/configured. Existing unrelated deletion entries remain unstaged.

- [ ] **Step 5: Record evidence and commit verification notes.**

Write the exact commands, platform versions, smoke-test results, and residual-scan exceptions to `docs/superpowers/verification/2026-07-11-agent-manager-trimming.md`, then commit only that evidence file:

```bash
git add docs/superpowers/verification/2026-07-11-agent-manager-trimming.md
git commit -m "test: verify Agent Manager trimming"
```

## Acceptance Checklist

- [ ] The app opens directly to the existing lifecycle UI, not a provider/settings shell.
- [ ] Claude Code, Codex CLI, Gemini CLI, OpenCode, OpenClaw, and Hermes detection remains available.
- [ ] Single install, batch install, update, shell selection, conflict diagnosis, and existing error feedback remain available.
- [ ] Provider, proxy, routing, failover, usage, MCP, skills, prompts, sessions, workspace, and non-lifecycle OpenClaw/Hermes management are not reachable and have no registered dedicated commands.
- [ ] Updater plugin, upstream endpoint/public key, update commands, update UI, and `DatabaseUpgrade` updater path are removed.
- [ ] Deep-link frontend, handlers, plugin initialization, dependency, commands, and `ccswitch` scheme are removed.
- [ ] Product metadata visibly says Agent Manager; old CC Switch identity remains only where required for MIT attribution/source notice.
- [ ] No excluded startup worker/plugin/side effect remains.
- [ ] TypeScript, unit tests, renderer build, Rust formatting/check/tests, Windows/macOS build or smoke verification, and WSL shell verification are recorded.
- [ ] Existing unstaged deletions were never staged, restored, or committed.

## Self-Review Before Implementation Handoff

- [ ] Every spec section maps to at least one task: updater safety (Task 2), lifecycle extraction (Task 3), single page (Task 4), branding (Task 5), frontend/backend deletion (Tasks 6 and 8), startup side effects (Task 8), deep links (Task 9), OpenClaw/Hermes boundaries (Task 7), cleanup (Task 10), cross-platform verification (Task 11).
- [ ] No task introduces a replacement lifecycle engine, new agent manifest, new command generator, PATH mutation, or duplicate-removal behavior.
- [ ] Every code task has exact paths, interface names, test/compile commands, expected outcomes, and a focused commit step.
- [ ] `DatabaseUpgrade` and `AboutSection` updater coupling is handled explicitly rather than left as a broken reference.
- [ ] Deep-link removal covers frontend, Rust handlers, plugin initialization, Cargo dependency, Tauri scheme, and command registration.
- [ ] OpenClaw/Hermes lifecycle definitions in `misc.rs` are protected from broad feature-domain deletion.
- [ ] The plan preserves the repository’s existing unstaged deletion set and instructs workers to stage only intended files.
