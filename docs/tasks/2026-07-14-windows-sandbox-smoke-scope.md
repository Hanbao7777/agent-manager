# Windows Sandbox Smoke Scope

## Decision

Implement only a lean package smoke harness for this Windows 10 host. The tracked source is `scripts/windows-sandbox`; the legacy external harness that maps the whole `test` root read-write is never a fallback.

## Verified statically

- MSI, Portable ZIP, checksum material, and optional offline WebView2 input are staged per run and hashed.
- The generated `.wsb` maps only that run's `input` read-only and `sandbox-output` read-write.
- `control` is not mapped, mappings are checked for equality/parent-child overlap, and output starts empty.
- Network, clipboard, printer, audio input, video input, and vGPU are disabled.
- The run ID is explicit; completion is represented by a final `complete.json` record.
- A pass requires the operator to inspect the explicitly returned run ID and confirm `complete.json.status == passed` plus matching run ID, scenario/profile, and lowercase `manifest_sha256`. `launch.json` may retain `config_sha256` for host inspection, but it is not echoed into `complete.json` because that would create a circular hash. The launcher never scans or auto-collects evidence.
- The manifest hash is compact UTF-8 JSON over only `schema`, `run_id`, `scenario`, `network_mode`, and sorted `files` entries (`path`, `length`, `sha256`); `manifest_sha256` is excluded. The host and runner independently recompute it, while the runner rejects rooted/separator paths, case-insensitive duplicates, malformed hashes/lengths, missing files, length mismatches, and file hash mismatches before artifact/install steps.

## Execution subset

When an operator explicitly supplies `-Launch`, the Sandbox runner covers artifact/hash contract, offline WebView2 prerequisite installation when the staged installer exists, quiet MSI install, MSI launch/render evidence, MSI uninstall, and Portable launch/render evidence. A missing or failed step produces failure evidence; partial output is not a pass.

## Deferred and not claimed

This harness does not claim the full ten Windows installation-orchestrator scenarios. UAC accept/decline, disk-space guard, batch partial failure, full tool-install orchestration, and macOS native scenarios remain deferred. No Sandbox, MSI, Portable, WebView2, product, network, CI, release, or publishing execution was performed for this implementation.
