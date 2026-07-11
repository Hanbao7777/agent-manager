# Task 6 Report

## Goal

Remove frontend feature domains unreachable from the `App -> AgentLifecyclePage`
entry while retaining lifecycle settings calls, the window/theme/i18n/toast
shell, lifecycle tests, and shared UI needed by the retained page.

## Files Changed/deleted domains

- Reduced `src/lib/api/settings.ts` and `src/lib/api/index.ts` to lifecycle
  settings calls and their installation-report types.
- Kept the lifecycle page, lifecycle dialogs/rows, shell, selected UI
  primitives, platform/version/error helpers, and lifecycle icon surface.
- Deleted unreachable provider, proxy/failover, usage, session, workspace,
  MCP, skills, prompts, profiles, import/export, sync, deep-link, and
  non-lifecycle OpenClaw/Hermes frontend components, hooks, APIs, queries,
  schemas, presets, and icon catalog.
- Removed obsolete feature tests and MSW state/handlers; retained the App and
  lifecycle characterization tests plus the minimal Tauri test mock.
- Reduced all four locale files to retained shell and lifecycle keys.

## Key Changes

- The product surface remains a single lifecycle page with all six agent tools.
- OpenCode and Hermes lifecycle cards use the retained terminal icon fallback,
  avoiding the deleted provider icon catalog.
- Removed MCP-only error translation from the retained error utility.
- No Rust/backend files or `pnpm-workspace.yaml` were modified.

## Verification

- `./node_modules/.bin/tsc --noEmit`: passed.
- Focused App/lifecycle tests: 7 passed.
- `./node_modules/.bin/vitest run`: 15 tests passed across 3 files.
- `./node_modules/.bin/vite build`: passed.
- Residue scans for excluded component names and excluded frontend imports:
  no matches.
- `pnpm typecheck`, `pnpm test:unit`, and `pnpm build:renderer` could not be
  invoked because `pnpm` is not installed; equivalent local binaries were run.

## Risks / Limitations

- The full historical feature test matrix was removed with the deleted feature
  domains; only retained-surface tests remain.
- Build emits existing dependency/browser-data and chunk-size warnings.

## Self-Review

- Confirmed the retained source graph contains only the lifecycle page, shell,
  required UI primitives, lifecycle settings API, and shared helpers.
- Confirmed focused tests cover App reachability, six-tool loading, install,
  diagnosis, upgrade confirmation, batch continuation, conflict rendering,
  and error toast behavior.
- Confirmed no Rust/backend or workspace manifest files are in the Task 6
  staging scope.

## Status

DONE

## commit/scope

Commit: `refactor: remove excluded frontend features`

Scope: frontend source, retained-surface tests/test setup, locale resources,
and this report only. Inherited deletions elsewhere in the worktree were not
staged.
