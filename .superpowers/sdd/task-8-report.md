# Task 8 Report

## Retained/Deleted Matrix

| Surface | Result | Notes |
| --- | --- | --- |
| `get_tool_versions` | Retained | Frontend lifecycle page invokes it; six-tool list and WSL/path probing remain in `misc.rs`. |
| `run_tool_lifecycle_action` | Retained | Install/update command planning, shell selection, WSL handling, and process error semantics remain unchanged. |
| `probe_tool_installations` | Retained | Conflict detection and anchored command planning remain unchanged. |
| `set_window_theme` | Retained | Frontend theme provider invokes it. |
| Clipboard command | Removed from registration | `clipboard.ts` has no caller; browser clipboard fallback remains frontend-only. |
| Other Tauri commands | Removed from registration | Provider, proxy, usage, MCP, prompt, profile, skills, sessions, workspace, sync, auth, and lightweight commands are no longer in `invoke_handler`. |
| Startup database/AppState | Removed | No database initialization, migrations, `AppState::new`, or managed state construction. |
| Startup workers and restores | Removed | No proxy restore, usage sync/backfill, WebDAV/S3 workers, provider/MCP/skills/prompt migrations, or startup synchronization. |
| Tray startup | Removed | No tray construction, menu refresh, or tray usage worker. The failover-only tray refresh was removed from unreachable failover code. |
| Deep links | Retained for Task 9 | Plugin registration, single-instance handling, URL parsing, and frontend events remain available. |
| Logging/window/process/platform support | Retained | Logging plugin, process/dialog/opener plugins, window state, Linux WebKit workaround, and platform app identity support remain. |

## Changes

- Reduced `commands/mod.rs` to `misc` lifecycle exports; removed the unused OAuth/Copilot state wrappers and their synchronization imports.
- Replaced `lib.rs` startup composition with the minimal retained runtime and four-command handler.
- Disabled `reqwest` default native-TLS features so retained Rustls networking does not pull an unavailable OpenSSL installation.
- Removed the stale Tauri updater capability permission left by inherited deletions.

## Verification

- `cargo fmt --manifest-path src-tauri/Cargo.toml --check`: passed after formatting.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed, with existing unreachable-module dead-code warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml`: passed, 1,794 tests passed and 2 ignored after removing the excluded integration-test suite.
- `corepack pnpm test:unit`: passed, 15 tests.
- `corepack pnpm typecheck`: passed.
- `corepack pnpm build:renderer`: passed.
- Source scan confirms `lib.rs` has exactly four registered commands and no database/state/worker/tray startup calls.
- Frontend invoke inventory is now exactly the four registered commands; the unused clipboard helper was removed.
- Deleted all 12 files under `src-tauri/tests/`; they covered only excluded provider, proxy, MCP, profile, skills, import/export, deep-link, and legacy configuration behavior.
- Removed stale proxy forwarder state-wrapper imports and kept the excluded proxy module out of the retained module graph.

## Risks / Limitations

- Deep-link support still compiles legacy import dependencies, so those modules remain present and generate dead-code warnings until Task 9 or a later dependency-pruning task.
- The existing `misc.rs` file still contains unregistered legacy helpers; only its lifecycle commands are reachable through Tauri.
