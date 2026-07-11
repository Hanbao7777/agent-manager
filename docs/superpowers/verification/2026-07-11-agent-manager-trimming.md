# Agent Manager Trimming Verification

Date: 2026-07-11
Base: `e4abcdf7ffa3a0e67accf3defb25e1d2dd27dccf`
Status: `DONE_WITH_CONCERNS`

## Environment

- Platform: Linux under WSL2, `x86_64`
- Kernel: `6.6.87.2-microsoft-standard-WSL2`
- Node: `v24.16.0`
- npm: `11.13.0`
- Corepack: `0.35.0`
- pnpm via Corepack: `11.10.0` (standalone `pnpm` is not installed)
- Rust: `rustc 1.95.0 (59807616e 2026-04-14)`
- Cargo: `1.95.0 (f2d3ce0bd 2026-03-21)`
- Rust toolchain: `1.95-x86_64-unknown-linux-gnu` from `rust-toolchain.toml`

## Gates

Commands were run from `apps/agent-manager`.

| Command | Result |
| --- | --- |
| `pnpm typecheck` | Could not start: standalone `pnpm` unavailable |
| `corepack pnpm typecheck` | PASS |
| `pnpm test:unit` | Could not start: standalone `pnpm` unavailable |
| `corepack pnpm test:unit` | PASS, 3 files and 15 tests |
| `pnpm build:renderer` | Could not start: standalone `pnpm` unavailable |
| `corepack pnpm build:renderer` | PASS, Vite production build |
| `pnpm format:check` | Could not start: standalone `pnpm` unavailable |
| `corepack pnpm format:check` | FAIL: `src/lib/api/settings.ts` is not Prettier-formatted |
| `cargo fmt --check` | PASS |
| `cargo check --manifest-path src-tauri/Cargo.toml` | PASS |
| `cargo test --manifest-path src-tauri/Cargo.toml` | PASS, 69 passed, 0 failed |
| `corepack pnpm install --frozen-lockfile --offline --ignore-scripts --lockfile-only` | PASS, lockfile already up to date |
| `cargo metadata --manifest-path src-tauri/Cargo.toml --locked --no-deps --format-version 1` | PASS |
| `git diff --check` | PASS |

Vitest emitted expected test stderr for mocked lifecycle failures and a stale browser-data warning; the test process passed. Vite emitted stale Browserslist data and a chunk-size warning; the build passed.

## Invoke and Registration Mapping

The four registered handlers in `src-tauri/src/lib.rs` map to the existing frontend/API calls:

- `get_tool_versions`: `src/lib/api/settings.ts`
- `run_tool_lifecycle_action`: `src/lib/api/settings.ts`
- `probe_tool_installations`: `src/lib/api/settings.ts`
- `set_window_theme`: `src/components/theme-provider.tsx`

No unregistered frontend invoke was found in the mapping scan.

## Residual Scans

The requested `rg` commands could not run because standalone `rg` is unavailable. Equivalent tracked-source `git grep` scans were run against `package.json`, `src`, and `src-tauri`.

Command:

```bash
git grep -n -i -E 'CC Switch|cc-switch|ccswitch|com\.ccswitch\.desktop|farion1231/cc-switch|latest\.json|tauri-plugin-updater|tauri-plugin-deep-link|ccswitch://|Provider|proxy|failover|usage|session|workspace|MCP|skill|prompt|deep.?link' -- package.json src src-tauri ':!src-tauri/Cargo.lock' ':!LICENSE' || true
```

The lifecycle/updater/deep-link/startup symbol scan returned no removed command or worker symbols. The remaining first-scan matches are reviewed exceptions or concerns:

- Upstream attribution and compatibility: `src-tauri/Cargo.toml` repository/baseline, legacy `.cc-switch` settings path in Windows compatibility settings, and `cc-switch-theme` local-storage key.
- Upstream compatibility code: the unused `src-tauri/src/proxy` module and its tests remain compiled; no proxy command is registered or invoked.
- Upstream request header: `cc-switch` User-Agent in `src-tauri/src/commands/misc.rs`.
- Generic Tauri WiX template variable `deep_link_protocols` remains in `src-tauri/wix/per-user-main.wxs`; no deep-link dependency, scheme, handler, or config registration remains.
- A few comments and embedded upstream resource text retain legacy terminology; no product metadata uses the old identity.

Product scan confirmed `Agent Manager` metadata and the six lifecycle tools: Claude Code, Codex CLI, Gemini CLI, OpenCode, OpenClaw, and Hermes. Old product identity matches are limited to the exceptions above.

## Linux Build

Initial `corepack pnpm tauri build --no-bundle` reached the Tauri CLI but failed because the configured `beforeBuildCommand` invokes unavailable standalone `pnpm`.

The supported no-bundle build then passed with the build hook overridden without changing repository configuration:

```bash
corepack pnpm exec tauri build --no-bundle --config '{"build":{"beforeBuildCommand":""}}'
```

Artifact evidence:

- `src-tauri/target/release/agent-manager`: 10,485,968 bytes, stripped x86-64 Linux ELF, dynamically linked
- `dist/index.html`: 403 bytes
- `dist/assets/index-Ca6FCM1r.js`: 510,304 bytes
- `dist/assets/index-Dozix3fL.css`: 28,678 bytes

No lifecycle install/update command was run, no agent was installed, and no user PATH or user configuration was changed.

## Platform Coverage

Windows and macOS disposable mocked lifecycle smoke tests are pending because this verification environment is Linux only. The required installed/missing/broken/update/no-op/batch partial-failure/conflict cases were covered by the existing 15 frontend characterization/unit tests, but no Windows or macOS application process was launched.

Windows WSL shell verification with a mocked `wsl.exe` is also pending. The current Linux WSL2 host is not evidence for Windows-host behavior.

The following remain pending and are not fabricated: Windows build/smoke verification, macOS build/smoke verification, and Windows-host WSL argument forwarding verification.

## Worktree Isolation

Before and after verification, there were no staged files. The inherited unstaged deletion set remained unchanged. Only this evidence file is intended for the verification commit; `.superpowers/sdd/task-11-report.md` is ignored.

## Concerns

1. The required format gate is red for `src/lib/api/settings.ts`; production code was not modified in this evidence task.
2. Compatibility/upstream proxy and legacy identity residues listed above should receive an explicit product-owner decision or cleanup task if the requirement is interpreted as a zero-residue scan.
