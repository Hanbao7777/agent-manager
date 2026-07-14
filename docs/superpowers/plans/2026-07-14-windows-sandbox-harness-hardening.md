# Windows Sandbox Harness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the unsafe broad writable Windows Sandbox test mapping with a versioned, fail-closed harness that stages immutable inputs and accepts only complete, provenance-validated evidence from one fresh per-run output directory.

**Architecture:** Keep all future harness source in `scripts/windows-sandbox`; its host launcher stages a selected tracked revision into external `D:\codex\ai-deploy-toolkit\test\input`, creates an unmapped host-owned `control` directory, and generates one profile-specific Sandbox configuration. The Sandbox runner revalidates the read-only manifest, works only under its local temporary directory, writes create-new evidence only to the mapped `sandbox-output` child, and writes `complete.json` last. Pester tests validate the contract without launching Windows Sandbox or product binaries; actual Sandbox and macOS interaction validation is separately gated.

**Tech Stack:** Windows PowerShell 5.1, Pester 5, Windows Sandbox `.wsb` XML, JSON Schema draft 2020-12, SHA-256, Git.

**Global Constraints:**

- Do not change Agent Manager product source, installers, artifacts, or product behavior.
- The external `D:\codex\ai-deploy-toolkit\test` directory is runtime storage, not a tracked source of truth; stage only an identified tracked revision into `test\input` and never map the test root.
- Each launch creates a new `test\evidence\run-id` directory, where `run-id` is the newly generated UUID, with unmapped `control` and newly empty `sandbox-output`; map only `test\input` read-only and that exact output child read-write.
- Default every descriptor to offline; enable networking only for the exact `network_mode: "online"` descriptor classification, and refuse a caller-selected profile that disagrees.
- Both profiles disable clipboard, printer, audio input, video input, and vGPU. Never map control records, a run container, a prior output directory, an input parent writable, the repository, or any parent containing another mapping.
- Reject traversal, duplicate normalized names, reparse points/symlinks, non-contained paths, hash mismatch, unexpected inputs, stale outputs, evidence collisions, incomplete evidence, timeout, and cleanup failure. Preserve partial evidence and report `failed` or `blocked`; never infer a pass or broadly kill processes.
- The 10 Windows scenarios retain their declared profile routes. The 9 macOS Intel and 9 macOS Apple Silicon scenarios are platform-specific `BLOCKED` until observed in real disposable macOS environments; hash, ZIP, Mach-O, source, fixture, and static checks are never interactive acceptance evidence.

---

## Locked File Tree And Responsibilities

```text
scripts/windows-sandbox/
  README.md                              # Runtime contract, profile policy, safe migration, rollback, and future-only execution instructions.
  Invoke-AgentManagerSandbox.ps1         # Host-only staging, validation, run creation, WSB generation, launch, collection, and owned-process cleanup.
  Invoke-SandboxRunner.ps1               # Sandbox-only manifest verification, scenario process control, evidence production, and finalization.
  lib/Harness.Common.psm1                 # Shared canonical path, reparse, hashing, JSON, XML, create-new, and process helpers.
  templates/agent-manager-sandbox-offline.wsb.xml # Versioned offline XML template with exactly two mapping slots.
  templates/agent-manager-sandbox-online.wsb.xml  # Versioned online XML template with exactly two mapping slots.
  schemas/scenario.schema.json           # Scenario descriptor wire contract and default-offline classification.
  schemas/input-manifest.schema.json     # Canonical immutable staged-input manifest contract.
  schemas/complete.schema.json           # Final evidence marker and fresh-hash contract.
  scenarios/windows/*.json                # Ten Windows descriptor files and their explicit profile routes.
  scenarios/macos/matrix.json             # Eighteen macOS records, scoped blocked/static only.
  tests/Harness.Common.Tests.ps1          # Unit tests for containment, reparse rejection, hashes, create-new, and XML validation.
  tests/Launcher.Preflight.Tests.ps1      # Host staging, mapping, profile, run container, manifest, timeout, and collector failure tests.
  tests/Runner.Evidence.Tests.ps1         # Sandbox-runner fixture tests for preflight, collisions, completion ordering, and owned cleanup.
  tests/Scenario.Matrix.Tests.ps1         # Exact 10 Windows plus 18 macOS matrix and execution-boundary assertions.
  tests/fixtures/input/                  # Harmless tracked runner/artifact/checksum fixtures used only by Pester.
  tests/fixtures/evidence/               # Harmless complete/incomplete/collision fixture records used only by Pester.
```

All paths above are future tracked repository paths. Runtime-only external paths are `D:\codex\ai-deploy-toolkit\test\input` and `D:\codex\ai-deploy-toolkit\test\evidence`; neither is added to Git, and no test maps, launches, or executes their legacy `.wsb`, legacy runner, installer, MSI, EXE, or WebView installer.

### Task 1: Define Shared Safe Filesystem And Record Primitives

**Files:**
- Create: `scripts/windows-sandbox/lib/Harness.Common.psm1`
- Create: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`

**Consumes:** PowerShell 5.1 filesystem APIs and fixture paths.
**Produces:** `Assert-ContainedPath`, `Assert-NoReparsePath`, `Get-DeterministicFileManifest`, `Write-JsonCreateNew`, `Get-Sha256Hex`, and `Assert-GeneratedConfiguration`.

- [ ] **Step 1: Write failing safety tests.**

```powershell
Import-Module "$PSScriptRoot/../lib/Harness.Common.psm1" -Force

Describe 'Assert-ContainedPath' {
  It 'rejects a separator-prefix sibling' {
    { Assert-ContainedPath -Root 'C:\work\input' -Candidate 'C:\work\input-old\x' } |
      Should -Throw '*outside root*'
  }
}
Describe 'Write-JsonCreateNew' {
  It 'never overwrites existing evidence' {
    $path = Join-Path $TestDrive 'started.json'
    [IO.File]::WriteAllText($path, '{"old":true}')
    { Write-JsonCreateNew -Path $path -Value @{ run_id = 'r1' } } |
      Should -Throw '*already exists*'
    [IO.File]::ReadAllText($path) | Should -Be '{"old":true}'
  }
}
```

- [ ] **Step 2: Run the focused tests and verify red.**

Run from `D:\codex\ai-deploy-toolkit\apps\agent-manager`:

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed
```

Expected: FAIL because the module and exported functions do not exist.

- [ ] **Step 3: Implement canonical containment, reparse rejection, deterministic hashing, and exclusive writes.**

```powershell
function Assert-ContainedPath {
  param([string]$Root, [string]$Candidate)
  $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd('\') + '\'
  $candidateFull = [IO.Path]::GetFullPath($Candidate)
  if (-not $candidateFull.StartsWith($rootFull, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Candidate is outside root: $candidateFull"
  }
  return $candidateFull
}
function Assert-NoReparsePath {
  param([string]$Path)
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point rejected: $Path" }
}
function Write-JsonCreateNew {
  param([string]$Path, [hashtable]$Value)
  $bytes = [Text.Encoding]::UTF8.GetBytes(($Value | ConvertTo-Json -Depth 16 -Compress))
  $stream = [IO.File]::Open($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
  try { $stream.Write($bytes, 0, $bytes.Length) } finally { $stream.Dispose() }
}
function Get-Sha256Hex { param([string]$Path) (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
```

Walk every ancestor between a declared root and candidate with `Get-Item -Force` and reject reparse points before reading, copying, deleting, or hashing. Enumerate files with ordinal normalized `/` relative names, reject duplicates and unlisted names, then hash sorted UTF-8 manifest lines; never use locale-dependent enumeration or `Remove-Item -Recurse` on an unvalidated path.

- [ ] **Step 4: Run focused tests and static syntax validation.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed
powershell.exe -NoProfile -Command "Import-Module .\scripts\windows-sandbox\lib\Harness.Common.psm1 -Force"
```

Expected: PASS; test coverage includes separator-aware containment, traversal, reparse rejection, deterministic hash ordering, exclusive create-new, and malformed XML rejection.

- [ ] **Step 5: Commit the shared primitives.**

```powershell
git add scripts/windows-sandbox/lib/Harness.Common.psm1 scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: add sandbox harness safety primitives"
```

### Task 2: Add Versioned Profile Templates And Schema Contracts

**Files:**
- Create: `scripts/windows-sandbox/templates/agent-manager-sandbox-offline.wsb.xml`
- Create: `scripts/windows-sandbox/templates/agent-manager-sandbox-online.wsb.xml`
- Create: `scripts/windows-sandbox/schemas/scenario.schema.json`
- Create: `scripts/windows-sandbox/schemas/input-manifest.schema.json`
- Create: `scripts/windows-sandbox/schemas/complete.schema.json`
- Modify: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`

**Consumes:** shared XML validation helpers.
**Produces:** exact online/offline XML profiles and validated scenario, manifest, and completion record interfaces.

- [ ] **Step 1: Write failing profile and schema assertions.**

```powershell
It 'uses exactly the required offline device and networking policy' {
  [xml]$xml = Get-Content "$PSScriptRoot/../templates/agent-manager-sandbox-offline.wsb.xml"
  $xml.Configuration.Networking | Should -Be 'Disable'
  @($xml.Configuration.MappedFolders.MappedFolder).Count | Should -Be 2
  foreach ($node in 'ClipboardRedirection','PrinterRedirection','AudioInput','VideoInput','VGpu') {
    $xml.Configuration.$node | Should -Be 'Disable'
  }
}
It 'defaults an omitted network mode to offline in launcher validation' {
  (Get-ScenarioProfile -Scenario @{ id = 'path-refresh' }) | Should -Be 'offline'
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`
Expected: FAIL because templates and `Get-ScenarioProfile` are absent.

- [ ] **Step 3: Create safe XML templates and JSON schemas.**

Use XML values that Windows Sandbox actually supports; only template tokens are replaced by XML APIs, never string concatenation:

```xml
<Configuration>
  <VGpu>Disable</VGpu><Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection><PrinterRedirection>Disable</PrinterRedirection>
  <AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput>
  <MappedFolders><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders>
  <LogonCommand><Command></Command></LogonCommand>
</Configuration>
```

The online template is byte-for-byte equivalent except `<Networking>Enable</Networking>` and `{{PROFILE}}` resolves to `online`. Make `network_mode` an enum of `online` and `offline`; the launcher interprets absence as offline and rejects every other value. Require `run_id`, `scenario_id`, profile, manifest/configuration hashes, UTC timestamps, declared evidence hashes, status, and `finalized_at_utc` in `complete.json`.

- [ ] **Step 4: Run static profile/schema tests.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`
Expected: PASS; both profiles have exactly two distinct sandbox paths, read-only input, read-write output, no host-path overlap, required disabled devices, and only the declared network difference.

- [ ] **Step 5: Commit contracts.**

```powershell
git add scripts/windows-sandbox/templates scripts/windows-sandbox/schemas scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: define sandbox profile and evidence contracts"
```

### Task 3: Encode The Authoritative Scenario Boundary

**Files:**
- Create: `scripts/windows-sandbox/scenarios/windows/clean-install.json`
- Create: `scripts/windows-sandbox/scenarios/windows/uac-accept.json`
- Create: `scripts/windows-sandbox/scenarios/windows/uac-decline.json`
- Create: `scripts/windows-sandbox/scenarios/windows/path-refresh.json`
- Create: `scripts/windows-sandbox/scenarios/windows/multiple-node-installations.json`
- Create: `scripts/windows-sandbox/scenarios/windows/proxy-failure.json`
- Create: `scripts/windows-sandbox/scenarios/windows/file-lock.json`
- Create: `scripts/windows-sandbox/scenarios/windows/disk-space-guard.json`
- Create: `scripts/windows-sandbox/scenarios/windows/batch-partial-failure.json`
- Create: `scripts/windows-sandbox/scenarios/windows/postflight-path-version.json`
- Create: `scripts/windows-sandbox/scenarios/macos/matrix.json`
- Create: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** scenario schema and the authoritative 28-case matrix.
**Produces:** ten executable Windows descriptors with explicit routing and eighteen macOS blocked/static records.

- [ ] **Step 1: Write the failing exact-matrix test.**

```powershell
$windows = Get-ChildItem "$PSScriptRoot/../scenarios/windows" -Filter *.json | ForEach-Object { Get-Content $_ | ConvertFrom-Json }
$macos = (Get-Content "$PSScriptRoot/../scenarios/macos/matrix.json" | ConvertFrom-Json).scenarios
@($windows).Count | Should -Be 10
@($macos | Where-Object architecture -eq 'intel').Count | Should -Be 9
@($macos | Where-Object architecture -eq 'apple_silicon').Count | Should -Be 9
@($macos | Where-Object { $_.execution_status -ne 'blocked' -or $_.evidence_kind -ne 'static_only' }).Count | Should -Be 0
@($windows | Where-Object { $_.network_mode -notin @('online','offline') }).Count | Should -Be 0
```

- [ ] **Step 2: Run the matrix test and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`
Expected: FAIL because no scenario descriptors exist.

- [ ] **Step 3: Create descriptors with routing and non-interactive macOS truth.**

```json
{
  "id": "proxy-failure",
  "platform": "windows",
  "network_mode": "online",
  "timeout_seconds": 180,
  "required_evidence": ["results.tsv", "sandbox-transcript.txt", "provenance.json"],
  "declared_process": "fixture-proxy-failure.ps1"
}
```

Route only descriptors that genuinely exercise approved download-success or download-failure behavior to `online`; use `offline` for clean install, UAC accept/decline, PATH refresh, multiple Node installations, file lock, disk-space guard, batch partial failure, and postflight path/version. Put the exact nine Intel and nine Apple Silicon names from the authoritative verification matrix in `matrix.json`, with `execution_status: "blocked"`, `evidence_kind: "static_only"`, and `requires_disposable_macos: true`.

- [ ] **Step 4: Run matrix tests.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`
Expected: PASS; tests fail if a macOS static/hash/ZIP/Mach-O observation is marked interactive or if a Windows descriptor has an implicit online route.

- [ ] **Step 5: Commit scenario declarations.**

```powershell
git add scripts/windows-sandbox/scenarios scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "feat: declare sandbox scenario profiles"
```

### Task 4: Implement Immutable Staging And Fresh Host-Owned Run Creation

**Files:**
- Create: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/input/runner.txt`
- Create: `scripts/windows-sandbox/tests/fixtures/input/artifact.txt`
- Create: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** source revision, schemas, descriptor, shared primitives, and external runtime roots passed as parameters.
**Produces:** atomically staged immutable input set, canonical manifest in both control and input, a new UUID-named run container with `control` and empty `sandbox-output`, and `control\launch.json`.

- [ ] **Step 1: Write failing staging/run-container tests.**

```powershell
It 'refuses a pre-existing run container without deleting it' {
  $run = Join-Path $TestDrive 'evidence/11111111-1111-1111-1111-111111111111'
  New-Item -ItemType Directory -Path $run -Force | Out-Null
  { New-HarnessRun -EvidenceRoot (Join-Path $TestDrive 'evidence') -RunId ([guid]::Parse((Split-Path $run -Leaf))) } | Should -Throw '*already exists*'
  Test-Path $run | Should -BeTrue
}
It 'rejects an extra staged file after manifest construction' {
  $input = New-TestInput -Root $TestDrive
  [IO.File]::WriteAllText((Join-Path $input 'unexpected.bin'), 'x')
  { Assert-InputManifest -InputRoot $input -ManifestPath (Join-Path $input 'input-manifest.json') } | Should -Throw '*unexpected*'
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: FAIL because host staging and `New-HarnessRun` do not exist.

- [ ] **Step 3: Implement staging and run creation with no unsafe deletion.**

Use `[guid]::NewGuid().ToString()` once per invocation, `Directory.CreateDirectory` only after containment/reparse checks, and `FileMode.CreateNew` for every control record. Stage a selected source revision into a sibling fresh temporary directory, validate its allowlist and hashes, then use `Move-Item` only to replace a previously validated dedicated `test\input` directory; if atomic replacement is unavailable, fail rather than copy into an existing unvalidated input root. The manifest contains normalized relative path, byte length, lowercase SHA-256, scenario ID, network mode, source revision, and its deterministic SHA-256; write an identical copy to `control\input-manifest.json` and staged `input-manifest.json`.

```powershell
$runId = [guid]::NewGuid().ToString()
$run = Join-Path $EvidenceRoot $runId
if (Test-Path -LiteralPath $run) { throw "Run container already exists: $run" }
[IO.Directory]::CreateDirectory((Join-Path $run 'control')) | Out-Null
[IO.Directory]::CreateDirectory((Join-Path $run 'sandbox-output')) | Out-Null
if ((Get-ChildItem -LiteralPath (Join-Path $run 'sandbox-output') -Force).Count -ne 0) { throw 'sandbox-output is not empty' }
```

- [ ] **Step 4: Run preflight tests.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: PASS; coverage proves malformed/duplicate run ID refusal, no prior-evidence overwrite, empty-output requirement, manifest extra/missing/hash mismatch rejection, source revision recording, reparse rejection, and immutable test-root separation.

- [ ] **Step 5: Commit staging safety.**

```powershell
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/fixtures/input scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: stage immutable sandbox inputs"
```

### Task 5: Generate, Validate, And Launch Only Matched Profiles

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** staged manifest, new run container, profile templates, and scenario descriptor.
**Produces:** validated `control\agent-manager-sandbox-offline.wsb` or `control\agent-manager-sandbox-online.wsb`, `control\launch.json`, and a tracked launched Sandbox process ID.

- [ ] **Step 1: Add failing mapping/profile mismatch tests.**

```powershell
It 'refuses online launch for an offline descriptor before process start' {
  { New-SandboxConfiguration -Profile online -Scenario @{ network_mode = 'offline' } -InputRoot 'C:\test\input' -OutputRoot 'C:\test\evidence\r\sandbox-output' -RunId 'r' } |
    Should -Throw '*profile does not match*'
}
It 'rejects nested host mappings and control mappings' {
  { Assert-NonOverlappingMappings -InputRoot 'C:\test\input' -OutputRoot 'C:\test\input\evidence' -ControlRoot 'C:\test\evidence\r\control' } | Should -Throw '*overlap*'
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: FAIL because configuration generation and mapping validation are absent.

- [ ] **Step 3: Implement XML DOM generation and strict mapping validation.**

```powershell
function Assert-NonOverlappingMappings {
  param([string]$InputRoot, [string]$OutputRoot, [string]$ControlRoot)
  foreach ($pair in @(@($InputRoot,$OutputRoot), @($InputRoot,$ControlRoot), @($OutputRoot,$ControlRoot))) {
    $a = [IO.Path]::GetFullPath($pair[0]).TrimEnd('\') + '\'; $b = [IO.Path]::GetFullPath($pair[1]).TrimEnd('\') + '\'
    if ($a.StartsWith($b,[StringComparison]::OrdinalIgnoreCase) -or $b.StartsWith($a,[StringComparison]::OrdinalIgnoreCase)) { throw "Mapping overlap: $($pair[0]) and $($pair[1])" }
  }
}
```

Load the selected XML with `[xml]`, set only the two `HostFolder` text nodes and the `LogonCommand.Command` text node through the DOM, save to `control`, reload it, and assert the exact schema values and two mappings. The generated command is `powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\AgentManagerHarness\Input\Invoke-SandboxRunner.ps1 -ScenarioPath C:\AgentManagerHarness\Input\scenario.json -RunId $runId -Profile $profile`, where the DOM XML-escapes the actual generated UUID and selected profile. Record a SHA-256 of the generated XML in create-new `control\launch.json` before `Start-Process -FilePath $wsbPath -PassThru`; never start a process before all checks pass. Default `Get-ScenarioProfile` to offline, allow only exact `online`, and record profile, scenario ID, manifest hash, config hash, source revision, launcher version, UTC start, and launched process ID.

- [ ] **Step 4: Run tests and static validation.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
powershell.exe -NoProfile -File .\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -Help
```

Expected: PASS; no Sandbox is launched by Pester or the help command. Tests prove both XML profiles have correct WSB schema, paths are non-overlapping/non-parental, control/run/evidence roots are unmapped, and mismatch refusal occurs before `Start-Process`.

- [ ] **Step 5: Commit profile generation.**

```powershell
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: generate validated sandbox profiles"
```

### Task 6: Implement Sandbox Preflight, Evidence, Timeout, And Owned Cleanup

**Files:**
- Create: `scripts/windows-sandbox/Invoke-SandboxRunner.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/evidence/required.txt`
- Create: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`

**Consumes:** read-only staged input/manifest/descriptor, `run-id`, profile, and read-write sandbox output mapping.
**Produces:** create-new `started.json`, `provenance.json`, `results.tsv`, transcript/log/screenshots as declared, and final `complete.json`; only launched child PIDs are eligible for cleanup.

- [ ] **Step 1: Write failing runner evidence and cleanup tests.**

```powershell
It 'does not write complete.json when a declared evidence file is absent' {
  $result = Invoke-RunnerFinalize -EvidenceRoot $TestDrive -RequiredEvidence @('results.tsv','screenshot.png') -RunId 'r1'
  $result.status | Should -Be 'blocked'
  Test-Path (Join-Path $TestDrive 'complete.json') | Should -BeFalse
}
It 'stops only a process it launched' {
  $stopped = @(); Stop-OwnedProcesses -ProcessIds @(41) -StopProcess { param($id) $stopped += $id }
  $stopped | Should -Be @(41)
  $stopped | Should -Not -Contain 'agent-manager'
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed`
Expected: FAIL because runner functions do not exist.

- [ ] **Step 3: Implement fail-closed runner behavior.**

At startup re-hash all manifest entries under `C:\AgentManagerHarness\Input`, reject extra/missing inputs, verify descriptor/profile/run ID, and emit a minimal create-new failure record then exit nonzero. Use only `$env:TEMP\AgentManagerSandbox\$RunId` for extraction and installer work. Start only descriptor-declared processes with `Start-Process -PassThru`, retain IDs, apply the descriptor's positive bounded `timeout_seconds`, and on timeout/failure stop only those numeric IDs, wait with a bounded deadline, and record cleanup failure.

```powershell
if (-not $process.WaitForExit($timeoutSeconds * 1000)) {
  Write-JsonCreateNew -Path (Join-Path $EvidenceRoot 'failure.json') -Value @{ run_id=$RunId; status='blocked'; reason='timeout' }
  foreach ($id in $ownedProcessIds) { Stop-Process -Id $id -ErrorAction SilentlyContinue }
  throw "Scenario timed out after $timeoutSeconds seconds"
}
foreach ($name in $RequiredEvidence) { if (-not (Test-Path -LiteralPath (Join-Path $EvidenceRoot $name)) -or (Get-Item (Join-Path $EvidenceRoot $name)).Length -eq 0) { throw "Required evidence missing: $name" } }
Write-JsonCreateNew -Path (Join-Path $EvidenceRoot 'complete.json') -Value @{ run_id=$RunId; status='complete'; finalized_at_utc=(Get-Date).ToUniversalTime().ToString('o'); files=$freshHashes }
```

Write `complete.json` only after all required records are present, non-empty where required, and freshly hashed; never use `-Force` for evidence writes. Remove only the validated sandbox-local temporary directory in `finally`; never delete input, evidence output, control, or a run container.

- [ ] **Step 4: Run runner tests.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed`
Expected: PASS; coverage includes tampered/extra/missing input, profile mismatch, evidence collision, failed transcript, timeout, non-owned-process protection, failed cleanup, missing evidence, fresh hashes, and complete-marker-last ordering.

- [ ] **Step 5: Commit runner containment.**

```powershell
git add scripts/windows-sandbox/Invoke-SandboxRunner.ps1 scripts/windows-sandbox/tests/fixtures/evidence scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1
git commit -m "feat: finalize isolated sandbox evidence"
```

### Task 7: Collect Evidence From Canonical Control Records Only

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** unmapped `control\launch.json`, `control\input-manifest.json`, generated WSB hash, and exactly the named `sandbox-output`.
**Produces:** create-new `control\collection.json` with accepted, failed, or blocked status while preserving all partial evidence.

- [ ] **Step 1: Write failing collector integrity tests.**

```powershell
It 'rejects a complete marker whose manifest hash differs from control' {
  $control = @{ run_id='r1'; profile='offline'; input_manifest_hash='aa'; configuration_hash='bb'; source_revision='abc' }
  $complete = @{ run_id='r1'; profile='offline'; input_manifest_hash='cc'; configuration_hash='bb'; source_revision='abc'; status='complete' }
  { Assert-CollectedEvidence -Control $control -Complete $complete } | Should -Throw '*manifest hash*'
}
It 'does not search sibling evidence for a passing result' {
  { Get-RunOutputPath -LaunchRecord @{ output_relative_path='../other/sandbox-output' } -RunContainer 'C:\evidence\r1' } | Should -Throw '*outside root*'
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: FAIL because the collector is absent.

- [ ] **Step 3: Implement strict collection and bounded host timeout.**

The launcher waits only for its `Start-Process -PassThru` Sandbox PID until a documented launch-to-completion deadline. On deadline expiry, record `blocked` in create-new `control\collection.json`, stop only that launched PID if still alive, record cleanup failure if it cannot be stopped, and leave the run container intact. Read only `sandbox-output` calculated from the canonical unmapped launch record, verify all run/profile/scenario/manifest/config/source-revision values and UTC timestamps within the documented 120-second clock-skew allowance, then accept only a schema-valid final `complete.json` with matching fresh hashes.

- [ ] **Step 4: Run collector tests.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: PASS; stale sibling output, malformed/non-final completion, wrong run/profile/scenario/hash/revision, timestamp violation, crash, timeout, and cleanup failure are rejected as failed/blocked without deletion or scanning.

- [ ] **Step 5: Commit collection policy.**

```powershell
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: validate sandbox evidence provenance"
```

### Task 8: Document Migration, Retirement, Rollback, And Future Gated Execution

**Files:**
- Create: `scripts/windows-sandbox/README.md`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Modify: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** the tracked harness revision and authoritative scenario matrix.
**Produces:** operator-safe migration/retirement/rollback procedure and future-only executable evidence validation/archive instructions.

- [ ] **Step 1: Write failing documentation contract tests.**

```powershell
It 'documents no-launch retirement and safe rollback invariants' {
  $readme = Get-Content "$PSScriptRoot/../README.md" -Raw
  $readme | Should -Match 'Do not launch.*agent-manager-test\.wsb'
  $readme | Should -Match 'Do not restore.*writable test-root mapping'
  $readme | Should -Match 'complete\.json'
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`
Expected: FAIL because the operator document does not exist.

- [ ] **Step 3: Write exact migration, retirement, rollback, and gated validation instructions.**

The README must state that migration inventories dependencies from the old external `D:\codex\ai-deploy-toolkit\test\run-agent-manager-sandbox.ps1` and `.wsb` by reading them only, stages required files from the tracked source revision, and retires the old files by moving them to a host archival location only after static preflight passes. It must explicitly prohibit launching either legacy file, restoring its writable `test` mapping, deleting evidence to retry, mapping external test-root parents, and treating static macOS output as interaction evidence.

Include these commands verbatim and label them **FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION**:

```powershell
# FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION
Set-Location D:\codex\ai-deploy-toolkit\apps\agent-manager
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId clean-install -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test

# FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId proxy-failure -Profile online -TestRoot D:\codex\ai-deploy-toolkit\test

# FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION
$run = Get-ChildItem D:\codex\ai-deploy-toolkit\test\evidence -Directory | Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
Expand-Archive (Join-Path $run.FullName 'sandbox-output\evidence.zip') -DestinationPath (Join-Path $run.FullName 'review')
```

Define rollback as selecting a previously validated tracked `scripts/windows-sandbox` Git revision in an isolated checkout, staging it to a fresh input directory/run ID, and proving offline completion with unmapped control and non-overlapping mappings. It must never roll back to the external unsafe harness. State acceptance criteria matching every design criterion, including no credentials in records, preservation of partial evidence, and separately archived Windows interaction versus macOS blocked/static reports.

- [ ] **Step 4: Run documentation and full static test suite.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests -Output Detailed
git diff --check
```

Expected: PASS; the suite launches no Sandbox, MSI, EXE, WebView installer, or product process and uses only fixtures/TestDrive.

- [ ] **Step 5: Commit migration and execution guidance.**

```powershell
git add scripts/windows-sandbox/README.md scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1 scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "docs: document sandbox harness migration"
```

### Task 9: Future-Only Disposable Platform Acceptance And Archive

**Files:**
- Create: `docs/superpowers/verification/2026-07-14-windows-sandbox-harness-hardening.md`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** completed implementation, immutable staged inputs, accepted run containers, and real disposable Windows/macOS environments.
**Produces:** a truthful verification record that separates real Windows interaction evidence from macOS blocked/static evidence.

- [ ] **Step 1: Run static preflight first.**

Run from `D:\codex\ai-deploy-toolkit\apps\agent-manager`:

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests -Output Detailed
git diff --check
```

Expected: PASS before any Sandbox launch; capture test version and exact harness Git revision in the verification record.

- [ ] **Step 2: Perform gated Windows execution on a disposable Windows Sandbox only.**

Run: the two commands labeled **FUTURE EXECUTION ONLY** in Task 8 for every one of the ten Windows descriptors, using each descriptor's declared profile.
Expected: each accepted result has a fresh unique run ID, unchanged input/control boundary, exactly one changed host `sandbox-output` child, matching launch/manifest/configuration/provenance hashes, required evidence, and final `complete.json`; offline cases have no network route and online download-failure records the classified failure without stale fallback.

- [ ] **Step 3: Validate evidence and archive without conflation.**

For every run, verify `control\launch.json`, `control\input-manifest.json`, generated WSB hash, `control\collection.json`, and output provenance/final marker; record timeout, crash, cleanup failure, or incomplete output as failed/blocked and retain it. Archive Windows screenshots/logs/transcript/results separately from the macOS matrix; do not claim hash, ZIP, Mach-O, source inspection, fixture tests, or Windows execution as macOS interaction evidence.

- [ ] **Step 4: Perform real macOS acceptance or retain blocks.**

Use separate disposable Intel and Apple Silicon macOS environments for their nine declared interactive cases each. Expected: each observed interaction has platform-native logs/screenshots and environment version/architecture; unavailable hardware remains explicitly `BLOCKED`, never pass by static equivalence.

- [ ] **Step 5: Verify rollback and commit the evidence record.**

Run the README rollback procedure in an isolated checkout against a prior validated tracked revision; expected: it rejects overlapping mappings and stale outputs, keeps control unmapped, and produces a complete offline evidence record without the old broad external harness. Then commit only the verification record:

```powershell
git add docs/superpowers/verification/2026-07-14-windows-sandbox-harness-hardening.md
git commit -m "docs: verify sandbox harness hardening"
```

## Plan Acceptance Criteria

- Static Pester tests prove safe canonical containment, reparse rejection, deterministic manifest/configuration hashing, create-new evidence behavior, profile mismatch refusal, exact WSB schema/device settings, no host mapping overlap, empty new output, manifest validation, timeout/owned-process cleanup, and complete-last collection validation.
- Future accepted Windows runs prove the exact two mappings, one fresh output child, unmapped host control provenance, offline-by-default profile selection, immutable staged inputs, and complete evidence matching canonical control records.
- The external unsafe `.wsb` and runner are read for migration inventory only, never launched as a fallback; rollback selects a prior validated tracked revision and maintains the hardened boundary.
- The verification archive lists ten Windows cases independently from eighteen macOS cases and requires real disposable Intel/Apple Silicon interaction evidence before any macOS acceptance.

## Plan Self-Review

- Spec coverage: Tasks 1-2 cover versioned source, safe paths, schemas, deterministic hashes, exact WSB device/network policy, and XML validation. Tasks 3-7 cover all 28 scenarios, immutable staging, non-overlapping mappings, control provenance, profile refusal, create-new/complete-last evidence, bounded ownership-only cleanup, collector validation, and fail-closed outcomes. Tasks 8-9 cover safe migration/retirement, rollback, static preflight, future gated Windows execution, archive, and real macOS-only interactive acceptance.
- Completeness scan: no unresolved entries, omitted implementation, prior-task references used as implementation instructions, or unbounded deletion/kill instructions remain. Every implementation and future verification path is exact.
- Consistency review: all tasks use `Invoke-AgentManagerSandbox.ps1`, `Invoke-SandboxRunner.ps1`, `control`, `sandbox-output`, `run_id`, `input_manifest_hash`, `configuration_hash`, `network_mode`, and `complete.json` consistently. Static tests are explicitly distinct from Sandbox and macOS interaction acceptance; runtime roots are external and never Git-tracked.

Plan complete and saved to `docs/superpowers/plans/2026-07-14-windows-sandbox-harness-hardening.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. Inline Execution - execute tasks in this session using executing-plans, batch execution with checkpoints.
