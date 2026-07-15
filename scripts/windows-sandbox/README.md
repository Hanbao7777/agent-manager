# Windows Sandbox Package Smoke Harness

This harness is a deliberately small, offline smoke check for the Windows package contract. It covers artifact/hash validation, an optional offline WebView2 prerequisite, quiet MSI install and uninstall, and Portable launch/render evidence. It does **not** claim the ten Windows installation-orchestrator scenarios or any macOS scenarios.

## Safety contract

- `Invoke-WindowsSandboxSmoke.ps1` prepares by default; real execution requires `-Launch`.
- Each run has an unmapped `control` directory, a read-only `input` mapping, and exactly one writable `sandbox-output` mapping.
- Networking, clipboard, printer, audio input, video input, and vGPU are disabled.
- Run IDs are explicit and never discovered by scanning for a latest evidence directory.
- Records use create-new semantics where the host controls them; `complete.json` is written last.
- Missing, malformed, failed, or incomplete completion records never pass.
- The legacy external broad `test`-root harness is not used.

The manifest contract hashes compact UTF-8 JSON containing only `schema`, `run_id`, `scenario`, `network_mode`, and sorted `files` entries (`path`, `length`, `sha256`). `manifest_sha256` is excluded from that payload. The host and runner independently reconstruct this payload; the runner also checks leaf-only unique paths, declared lengths, and file hashes before any artifact or installer step.

## Static preparation

```powershell
.\Invoke-WindowsSandboxSmoke.ps1 `
  -MsiPath D:\codex\ai-deploy-toolkit\test\Agent-Manager-0.1.0-Windows.msi `
  -PortableZipPath D:\codex\ai-deploy-toolkit\test\Agent-Manager-0.1.0-Windows-Portable.zip `
  -ChecksumsPath D:\codex\ai-deploy-toolkit\test\SHA256SUMS.txt `
  -WebView2Path D:\codex\ai-deploy-toolkit\test\MicrosoftEdgeWebView2RuntimeInstallerX64.exe `
  -PrepareOnly
```

Pass `-Launch` instead of `-PrepareOnly` only after reviewing the generated `control` records. This repository task does not launch Sandbox or product binaries.

An operator may call a run a pass only when the explicit run ID's `control\launch.json` and Sandbox `sandbox-output\complete.json` agree on run ID, scenario/profile, and the lowercase `manifest_sha256`, and `complete.json.status` is exactly `passed`. `launch.json` retains `config_sha256` for host inspection; it is not compared inside `complete.json` because doing so would create a circular configuration hash. Missing, stale, mismatched, or `failed` completion records are not passes; the host launcher does not collect results automatically.
