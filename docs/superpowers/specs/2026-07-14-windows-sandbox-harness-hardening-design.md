# Windows Sandbox Harness Hardening Design

## Background

The current Sandbox configuration maps the complete host `test` directory to `C:\AgentManagerTest` with `ReadOnly` set to `false` (`test/agent-manager-test.wsb:7-9`). Its logon command starts the mapped PowerShell script directly (`test/agent-manager-test.wsb:12-13`). The script treats that mapped root as both its source directory (`test/run-agent-manager-sandbox.ps1:3`) and the parent of its evidence directory (`:4`), creates that evidence directory (`:8`), and writes transcripts, TSV results, logs, and screenshots there (`:6`, `:11`, `:23`, `:35`, `:44`, `:105`, `:122`).

Consequently, a process in the Sandbox can modify not only evidence but also the harness script, `.wsb` adjacent material, artifacts, installers, checksums, and any other file under the host test root. The current configuration also enables networking, clipboard redirection, and default vGPU (`test/agent-manager-test.wsb:2-4`) for every invocation, including checks that do not need them. This design hardens the harness boundary without changing Agent Manager product behavior.

## Goal

Make Windows Sandbox test execution consume immutable, hash-validated staged inputs and produce durable, per-run host evidence through the smallest explicit writable boundary. Separate online and offline execution configurations so network access exists only for named scenarios that need it, and make invalid inputs, stale evidence, ambiguous output locations, and configuration mistakes fail closed.

### Definitions

- **Input root**: host `test\input`, containing only staged harness inputs required by a run: the runner script, selected artifact(s), checksum material, approved runtime prerequisite(s), scenario descriptor, and input manifest.
- **Evidence root**: host `test\evidence`, owned by the host-side launcher and containing only run containers.
- **Run container**: host `test\evidence\<run-id>`, where `<run-id>` is a newly generated UUID-like identifier created once by the host-side launcher. It is never mapped into the Sandbox and contains both host-controlled records and one Sandbox-output child.
- **Control directory**: host `test\evidence\<run-id>\control`, inside the unmapped run container. It holds the canonical input manifest, generated Sandbox configuration, launch record, and host collection result. The Sandbox cannot write this directory.
- **Sandbox-output directory**: host `test\evidence\<run-id>\sandbox-output`, a newly created empty child of the run container. It is the only host path writable from that run's Sandbox.
- **Online scenario**: a scenario explicitly marked `network_mode: online` in its staged scenario descriptor because it validates an approved download or network-failure path.
- **Offline scenario**: every scenario not explicitly marked online. It executes with Windows Sandbox networking disabled.
- **Controlled export**: a host-side post-execution collection step that copies only declared, completed evidence from a sandbox-local location after the Sandbox has exited. It is an alternative architecture, not the recommended approach.

## Non-Goals

- Change Agent Manager installation, lifecycle, networking, UI, artifact format, or product security behavior.
- Run Windows Sandbox, MSI, `agent-manager.exe`, WebView installers, GitHub Actions, release, or publishing operations as part of this design work.
- Make macOS interactive testing run inside Windows Sandbox or present static analysis as interaction evidence.
- Retain a compatibility mode that maps the complete host `test` root read-write.
- Add a general-purpose host-to-Sandbox file-transfer service.

## Scope

Future implementation is limited to Windows Sandbox harness configuration, host-side staging/launch policy, `test\input` organization, runner evidence behavior, and verification records. The product executable is an opaque test subject; only its documented test invocation is affected.

Conceptual paths are fixed as follows:

| Purpose | Host path | Sandbox path | Access from Sandbox |
| --- | --- | --- | --- |
| Immutable staged inputs | `D:\codex\ai-deploy-toolkit\test\input` | `C:\AgentManagerHarness\Input` | Read-only |
| Host-controlled run records | `D:\codex\ai-deploy-toolkit\test\evidence\<run-id>\control` | Not mapped | No Sandbox access |
| Isolated Sandbox output for one run | `D:\codex\ai-deploy-toolkit\test\evidence\<run-id>\sandbox-output` | `C:\AgentManagerHarness\Evidence` | Read-write |
| Ephemeral extracted files and installer work | none | `%TEMP%\AgentManagerSandbox\<run-id>` | Sandbox-local |

The host-side launcher creates `test\input` only through controlled staging, creates a fresh run container with separate `control` and empty `sandbox-output` children, and generates the selected `.wsb` from the matching online/offline template. The Sandbox never receives a writable mapping of `test`, `test\input`, the repository, the evidence root, a run container, the control directory, or a directory that contains another run's output.

Future harness source of truth is a tracked `scripts\windows-sandbox\` directory in this feature repository, alongside the existing tracked `scripts\` utilities. It will contain versioned online/offline `.wsb` templates, the host launcher, the Sandbox runner, schema/fixture files, and documentation; the launcher stages an identified revision of those files into the external `test\input` runtime location. The existing external `D:\codex\ai-deploy-toolkit\test` `.wsb` and PowerShell files are not in this Git repository and are neither a source of truth nor a rollback target.

## Constraints

- Inputs and evidence must never share a writable Sandbox mapping or a mapped parent path.
- A selected `.wsb` must map `test\input` read-only and exactly one newly created empty `test\evidence\<run-id>\sandbox-output` directory read-write at different Sandbox paths.
- The run container and `control` directory must remain unmapped and host-controlled for the full run; host records are never placed in `sandbox-output` before launch.
- Online networking is an opt-in scenario attribute; absence of the attribute means offline.
- Clipboard, printer, audio input, video input, and vGPU are disabled in both profiles unless a later written, scenario-specific exception documents why the test cannot run without one. No such exception is part of this design.
- Evidence must identify the scenario, run ID, selected profile, input-manifest hash, Sandbox configuration hash, timestamps, process outcomes, and completeness state without recording credentials or personal configuration.
- Failures to stage, hash, launch, write required evidence, or finalize a run must be recorded as failed or blocked, never inferred as passing.
- The existing 28-scenario matrix remains authoritative. This harness design changes execution containment and evidence collection, not scenario expectations.

## Proposed Approach

### Design Decision

Adopt **Alternative B: a dedicated `test\input` read-only directory plus one separately mapped per-run Sandbox-output directory**. It supplies a clear, practical Windows Sandbox boundary with ordinary mapped-folder behavior, avoids overlapping host mappings, preserves host-controlled provenance outside the writable mapping, and keeps output durable during execution without the export fragility of Alternative C.

### Alternatives

| Alternative | Boundary strength | Automation reliability | Operator complexity | Evidence durability | Migration cost | Decision |
| --- | --- | --- | --- | --- | --- | --- |
| A. Map `test` read-only and `test\evidence` read-write at distinct Sandbox paths | Moderate in intent, but dependent on ambiguous behavior when a child host path is mapped separately from its read-only parent | Moderate; overlapping/nested host mappings can be surprising and are difficult to prove portable | Low | High while the Sandbox is running | Low | Reject |
| B. Create `test\input` read-only and map only `test\evidence\<run-id>\sandbox-output` read-write | Strong practical separation: no writable mapped parent contains inputs or host control records | High; two non-overlapping mappings use clear access rules | Moderate; requires controlled input staging | High while the Sandbox is running and after teardown | Moderate | Recommend |
| C. No host-writable mapping; controlled export after Sandbox exit | Strongest execution isolation | Lower; export can fail on timeout, crash, missing files, or teardown before collection | High; requires collection channel and recovery protocol | Conditional; evidence remains ephemeral until export succeeds | High | Reject for the current harness |

Alternative A is deliberately not selected even though its Sandbox paths would differ. The host paths overlap (`test` contains `test\evidence`), so it leaves the policy dependent on nested-mapping semantics rather than an unambiguous filesystem boundary. Alternative C is appropriate only if a later threat model requires no host write during execution and funds a tested export/retry channel; it must not be substituted silently.

### Profile And Device Policy

Future implementation provides two separately named generated configurations:

- `agent-manager-sandbox-offline.wsb`: `<Networking>Disable</Networking>`.
- `agent-manager-sandbox-online.wsb`: `<Networking>Enable</Networking>`.

Both configurations set `<ClipboardRedirection>Disable</ClipboardRedirection>`, `<PrinterRedirection>Disable</PrinterRedirection>`, `<AudioInput>Disable</AudioInput>`, `<VideoInput>Disable</VideoInput>`, and `<VGpu>Disable</VGpu>`. They have the same two mappings and differ only in networking and profile identity. The logon command invokes the read-only runner from `C:\AgentManagerHarness\Input`; it must pass the selected scenario descriptor and `run-id`, and it must not derive either from a writable path.

The host-side launcher selects the online profile only when the scenario descriptor has the exact named online classification. Online scenarios include approved download-success and download-failure routing checks; failure injection is still preferred where it proves the condition. All checksum, MSI/portable launch, residue, lifecycle, PATH, shell, and static checks use the offline profile unless the authoritative scenario descriptor marks them online. If the requested profile and descriptor classification disagree, launch is refused.

### Staging, Validation, And Launch

Before launch, the host-side launcher performs these fail-closed steps:

1. Refuse a malformed, duplicate, or pre-existing target run container; generate another run ID rather than deleting or reusing evidence. Create `test\evidence\<run-id>\control` and `test\evidence\<run-id>\sandbox-output`; verify that `sandbox-output` is empty before launch while allowing only host records in `control`.
2. Stage a complete selected input set under `test\input` from the tracked `scripts\windows-sandbox\` source revision using an atomic replace into a fresh staging directory. The input set excludes `evidence` and excludes unneeded test-root files.
3. Generate a canonical input manifest in `control` that lists every staged file by normalized relative path, byte length, SHA-256, scenario identifier, scenario network mode, source revision, and manifest SHA-256; stage an identical read-only copy with the inputs. Refuse symlinks, reparse points, path traversal, duplicate normalized names, unexpected files, or hashes that do not match the selected artifact contract.
4. Generate the selected `.wsb` in `control` from the tracked template revision only after validating that its host mappings are non-overlapping: `test\input` and the exact `test\evidence\<run-id>\sandbox-output`. Refuse a source path equal to, parent of, or child of the other mapping, or any mapping of the run container or `control` directory.
5. Write the host-controlled launch record in `control` before launch, containing run ID, profile, scenario ID, canonical manifest hash, generated configuration hash, source revision, host launcher version, and UTC start time. The run container may now contain those control records, but `sandbox-output` remains empty until the Sandbox writes it.

At Sandbox startup, the runner re-hashes every manifest entry from `C:\AgentManagerHarness\Input`, rejects extra or missing staged files, verifies the declared scenario and profile, and writes a `started.json` record to `C:\AgentManagerHarness\Evidence`. It extracts artifacts only into `%TEMP%\AgentManagerSandbox\<run-id>`, never into either mapped input or evidence path. A failed preflight emits a minimal failure record if the evidence mapping is usable, then exits nonzero; it must never continue with unvalidated input.

### Evidence Isolation, Completeness, And Provenance

The evidence mapping is `C:\AgentManagerHarness\Evidence` to exactly `test\evidence\<run-id>\sandbox-output`, not the evidence root, run container, or control directory. The runner treats it as append-only for a run: it uses create-new semantics for named evidence files, refuses an existing `started.json`, `results.tsv`, transcript, screenshot, log, or final record, and cannot overwrite host control records or output from another run because neither is mapped.

Each run must contain a provenance record with the run ID, scenario ID, online/offline profile, Windows build, runner version/hash, input-manifest hash, configuration hash, UTC start/end times, and process exit results. `results.tsv`, transcript, MSI logs when applicable, declared screenshots, and a structured result record form the evidence set. The runner writes a `complete.json` marker last, after verifying that all scenario-required files exist, are non-empty where applicable, and have fresh hashes recorded in that marker. An absent, malformed, mismatched, or non-final `complete.json` means incomplete evidence and blocks result acceptance.

The host-side collector reads Sandbox evidence only from the `sandbox-output` child named by the unmapped `control\launch.json`, then compares its run ID, profile, scenario, manifest hash, configuration hash, and source revision against the canonical control records. It rejects timestamps outside the launch interval with a small documented clock-skew allowance and does not scan `test\evidence` for a convenient prior result. A timeout, Sandbox crash, transcript failure, missing final marker, mismatch with control records, or cleanup error is a failed or blocked run with preserved partial output; it cannot reuse stale evidence.

### Process, Timeout, And Cleanup Behavior

The runner starts only the scenario-declared process tree, captures child process IDs and exit status, and applies scenario-declared bounded timeouts. On timeout or failure it records the condition in `sandbox-output`, stops only processes it started, waits for them to exit, and records unsuccessful cleanup rather than broad process-name termination. Sandbox-local temporary directories are removed in `finally` after evidence finalization attempts; `sandbox-output` is never deleted by the runner or launcher, and the control directory remains host-owned. The host-side launcher imposes a launch-to-completion deadline, records a timeout outcome in `control` if the Sandbox does not finish, and preserves the run container for review.

### Scenario Routing And Platform Boundary

The authoritative matrix has 28 cases: 10 Windows cases, 9 macOS Intel cases, and 9 macOS Apple Silicon cases. The Windows harness executes the 10 Windows cases in the selected profile: clean install, UAC accept, UAC decline, PATH refresh, multiple Node installations, proxy failure, file lock, disk-space guard, batch partial failure, and postflight path/version. Cases requiring a real external download are named online; the rest are offline unless their descriptor says otherwise.

The 18 macOS cases are outside Windows Sandbox execution. The 9 Intel and 9 Apple Silicon cases remain `BLOCKED` until observed on their respective disposable macOS environments. The Windows harness may collect static evidence that the macOS cases are declared, scoped, and unexecuted, but static review, source inspection, fixture tests, or a Windows result must never be reported as macOS interaction evidence.

### Rollback

Rollback is a configuration/harness rollback, not a product rollback. Future implementation tracks the templates, launcher, runner, schemas, and staging contract in `scripts\windows-sandbox\`; each launch records that Git commit and content hashes in the unmapped control directory, then stages that exact revision into external `test\input`. If preflight, mapping validation, or evidence finalization defects prevent reliable testing, stop launches, preserve run containers, and select a previously validated tracked `scripts\windows-sandbox\` revision in an isolated checkout for a new staged run. Do not restore the current unsafe external writable-test-root harness as an emergency fallback. Rollback verification is successful only when the selected tracked revision again rejects overlapping mappings, keeps control records unmapped, refuses stale output directories, and produces a complete offline evidence record using immutable inputs.

## Risks

- Windows Sandbox mapped-folder behavior is a host integration boundary, not a content-integrity system; input manifest validation detects accidental or pre-launch tampering but does not replace trusted host staging.
- Disabling vGPU may affect rendering performance or graphics-dependent behavior. A later exception must be scenario-specific, written, and tested; the default remains disabled.
- Disabling network makes unclassified scenarios fail promptly rather than accidentally using the Internet. Scenario descriptors therefore need careful review during migration.
- A per-run evidence mapping protects sibling runs but host access controls still govern who can alter host evidence outside Sandbox execution.
- Evidence can be partial after Sandbox termination. The completion marker and strict collector prevent partial data from becoming a pass, at the cost of more blocked outcomes.
- Reorganizing inputs can uncover undocumented dependencies on files currently present in the broad `test` directory. That is a desired fail-closed discovery and must be resolved by explicitly staging the dependency, not by widening the map.

## Acceptance Criteria

- The implemented `.wsb` profiles map only `test\input` read-only and one non-overlapping, newly empty `test\evidence\<run-id>\sandbox-output` directory read-write at the conceptual paths defined in this document.
- Neither profile maps the test root, repository, evidence root, run container, control directory, another run output directory, or an input parent writable.
- Online networking is enabled only for a descriptor explicitly named online; all other scenarios launch offline, and profile mismatch fails before Sandbox launch.
- Clipboard, printer, audio input, video input, and vGPU are disabled in both profiles.
- Launch preflight verifies mapping non-overlap, input-manifest completeness and hashes, source revision, run-container uniqueness, empty `sandbox-output`, scenario identity, and profile identity; any failure prevents launch.
- Every accepted result has matching host-controlled launch/configuration/manifest records in `control` and Sandbox start, provenance, and final completion records in `sandbox-output`, tied to one run ID and input-manifest hash; stale, incomplete, or mismatched evidence is rejected.
- Timeout, process failure, cleanup failure, and evidence-write failure retain partial evidence and result in failure or blocked status, never pass.
- The Windows 10-case subset has an explicit online/offline route, while all 18 macOS cases retain their platform-specific blocked/static status until real macOS evidence exists.
- The implementation changes harness/config/input organization only and does not alter Agent Manager product behavior.
- Rollback can be verified without reintroducing a writable test-root mapping.

## Task Breakdown

The following is future implementation work and is not authorization to modify the harness now.

1. Add the future tracked `scripts\windows-sandbox\` source of truth for templates, launcher, runner, schemas, and staging documentation; version the staging contract without treating external `test` files as source-controlled.
2. Inventory the runner's required files, create controlled `test\input` staging from that tracked source revision, and define versioned scenario descriptors and input manifests.
3. Add host-side preflight and launch generation that creates a unique run container with unmapped `control` and empty `sandbox-output` children, validates non-overlapping mappings, and chooses the online or offline profile.
4. Replace the broad writable mapping with read-only inputs and only the `sandbox-output` child writable, then apply the shared device-redirection policy.
5. Update the runner to validate staged manifests, use sandbox-local temporary work, write create-new provenance/evidence records only to `sandbox-output`, and finalize `complete.json` last.
6. Encode the 10 Windows scenarios with explicit network modes and link the 18 macOS cases to their blocked/static evidence records without treating them as executed interactions.
7. Add automated tests for mapping rejection, control/output isolation, manifest tampering, profile mismatch, stale output, evidence collisions, timeout cleanup, completion-marker failure, and source-controlled rollback behavior.
8. Perform disposable Windows validation only after implementation approval; record each observed result and preserve incomplete evidence when a scenario is blocked or fails.

## Verification

Future validation must be performed after implementation approval and must not be inferred from this design document.

1. Run static configuration tests that parse both profiles and assert the exact two mappings, distinct Sandbox paths, non-overlapping host paths, unmapped run-container/control paths, input read-only flag, empty per-run `sandbox-output` write flag, disabled redirections, disabled vGPU, and profile-specific network setting.
2. Run host-side preflight tests for missing input, modified hash, manifest extra file, reparse point, profile mismatch, duplicate run container, non-empty `sandbox-output`, nested mapping, a mapped control directory, and launcher timeout.
3. Run runner tests using harmless fixtures to prove input re-hashing, control/output separation, evidence create-new behavior, manifest/provenance propagation, required-file completeness checks, final-marker ordering, and cleanup recording.
4. In a disposable Windows Sandbox, execute every Windows scenario in its declared profile and verify that only the selected `sandbox-output` child changes on the host. Confirm that an offline scenario has no network route and an online download-failure scenario records the classified failure without falling back to stale evidence.
5. Inspect each accepted run container for unmapped control records matching the source revision and for `sandbox-output` records with matching hashes, a final `complete.json`, declared artifacts, and no credentials. Mark any missing, partial, or mismatched case failed or blocked.
6. Verify rollback in an isolated checkout by selecting a prior validated tracked `scripts\windows-sandbox\` revision, confirming the old broad writable mapping is not reintroduced, control remains unmapped, and producing a complete offline evidence run.
7. Run `git diff --check`, inspect the changed-file list, and retain a verification report that separately lists Windows interaction evidence and macOS blocked/static evidence.
