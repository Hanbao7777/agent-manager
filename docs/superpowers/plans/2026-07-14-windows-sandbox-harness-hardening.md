# Windows Sandbox Harness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a versioned, fail-closed Windows Sandbox harness that stages hash-validated immutable inputs and accepts only complete evidence from one fresh, per-run writable output directory.

**Architecture:** Repository-owned source lives in `scripts/windows-sandbox`; the host launcher stages an identified revision into external runtime `D:\codex\ai-deploy-toolkit\test\input`, creates a UUID run container, and generates a profile-specific WSB under unmapped `control`. The Sandbox receives only read-only input and the fresh writable `sandbox-output` child, revalidates the manifest, works in sandbox-local `%TEMP%`, and writes its completion marker last. The host collector trusts only the unmapped canonical control records for that exact UUID and preserves every failed or partial run.

**Tech Stack:** Windows PowerShell 5.1, Pester 5, Windows Sandbox XML, JSON Schema draft 2020-12, SHA-256, Git.

**Global Constraints:**

- All repository commands in this plan run from `D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator`; never operate in the dirty main checkout.
- `D:\codex\ai-deploy-toolkit\test` is external runtime data, not source control. The tracked source of truth is only `scripts\windows-sandbox`; external `test\input` and `test\evidence` are never mapped as a parent or committed.
- A run ID is a newly generated GUID in canonical lowercase `D` form. Each run creates `test\evidence\<guid>\control` and an empty sibling `sandbox-output`; only the latter is writable in the Sandbox.
- Offline is the default. Only an exact descriptor value `network_mode: "online"` selects online; caller/profile disagreement is refused before configuration generation or process start.
- Both WSB profiles disable clipboard, printer, audio input, video input, and vGPU. The two host mappings are non-overlapping: `test\input` read-only at `C:\AgentManagerHarness\Input` and exactly one `sandbox-output` read-write at `C:\AgentManagerHarness\Evidence`.
- Reject reparse points, path traversal, prefix siblings, duplicate normalized names, unlisted inputs, hash mismatch, stale output, collisions, malformed records, non-final evidence, process/cleanup failure, and timeouts. Never broadly kill by process name, overwrite evidence, scan sibling runs, or delete a runtime path to retry.
- The authoritative matrix remains 10 Windows scenarios plus 9 macOS Intel and 9 macOS Apple Silicon scenarios. macOS fixture, hash, ZIP, Mach-O, source, and static observations are not interactive acceptance; each macOS scenario remains blocked until real disposable hardware records it.

---

## Locked File Tree And Responsibilities

```text
scripts/windows-sandbox/
  README.md                                      # Runtime boundary, read-only migration inventory, retirement, rollback, and future-only execution protocol.
  Invoke-AgentManagerSandbox.ps1                 # Host staging, run creation, profile generation, bounded launch, collection, and archive record.
  Invoke-SandboxRunner.ps1                       # Sandbox manifest preflight, declared-process execution, owned cleanup, and evidence finalization.
  lib/Harness.Common.psm1                         # Canonical paths, reparse guards, deterministic JSON/hash, schema/XML and exclusive-write primitives.
  templates/agent-manager-sandbox-offline.wsb.xml # Offline WSB DOM template.
  templates/agent-manager-sandbox-online.wsb.xml  # Online WSB DOM template.
  schemas/scenario.schema.json                    # Descriptor contract including network mode and bounded timeout.
  schemas/input-manifest.schema.json              # Immutable staged-file contract.
  schemas/complete.schema.json                    # Complete-marker/provenance and evidence-hash contract.
  scenarios/windows/clean-install.json
  scenarios/windows/uac-accept.json
  scenarios/windows/uac-decline.json
  scenarios/windows/path-refresh.json
  scenarios/windows/multiple-node-installations.json
  scenarios/windows/proxy-failure.json
  scenarios/windows/file-lock.json
  scenarios/windows/disk-space-guard.json
  scenarios/windows/batch-partial-failure.json
  scenarios/windows/postflight-path-version.json # The exact ten Windows profile-routed descriptors.
  scenarios/macos/matrix.json                     # Exact nine Intel and nine Apple Silicon blocked/static records.
  tests/Harness.Common.Tests.ps1                  # Primitive, XML, JSON, path, and deterministic manifest tests.
  tests/Launcher.Preflight.Tests.ps1              # Staging, profile, run-container, collector, and archive-record tests.
  tests/Runner.Evidence.Tests.ps1                 # Runner preflight, owned process-tree, timeout, cleanup, and complete-last tests.
  tests/Scenario.Matrix.Tests.ps1                 # Exact 10/18 scenario boundary and non-interactive macOS tests.
  tests/fixtures/input/                           # Harmless tracked files used by Pester only.
  tests/fixtures/evidence/                        # Harmless complete and incomplete JSON evidence used by Pester only.
```

### Task 1: Define Canonical Safety Primitives

**Files:**
- Create: `scripts/windows-sandbox/lib/Harness.Common.psm1`
- Create: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`

**Consumes:** PowerShell 5.1 filesystem and cryptography APIs.
**Produces:** `ConvertTo-CanonicalPath`, `Assert-ContainedPath`, `Assert-NoReparsePath`, `Assert-SafeTree`, `Get-Sha256Hex`, `ConvertTo-CanonicalJson`, `Write-JsonCreateNew`, `Read-JsonFile`, `Assert-RunId`, and `Get-DeterministicManifest`.

- [ ] **Step 1: Write failing path, reparse, run-ID, and deterministic-manifest tests.**

```powershell
Import-Module "$PSScriptRoot/../lib/Harness.Common.psm1" -Force
Describe 'harness safety primitives' {
  It 'rejects separator-prefix siblings' {
    { Assert-ContainedPath -Root 'C:\work\input' -Candidate 'C:\work\input-old\a.txt' } | Should -Throw '*outside root*'
  }
  It 'rejects invalid GUID text before it becomes a path' {
    { Assert-RunId -RunId '../old-run' } | Should -Throw '*canonical GUID*'
  }
  It 'does not overwrite a record' {
    $path = Join-Path $TestDrive 'started.json'; [IO.File]::WriteAllText($path, '{"old":true}')
    { Write-JsonCreateNew -Path $path -Value ([ordered]@{run_id='00000000-0000-0000-0000-000000000001'}) } | Should -Throw '*exists*'
    [IO.File]::ReadAllText($path) | Should -Be '{"old":true}'
  }
}
```

- [ ] **Step 2: Run the focused test and verify red.**

Run from `D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator`:

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed
```

Expected: FAIL because `Harness.Common.psm1` and its exported functions do not exist.

- [ ] **Step 3: Implement the primitive module.**

```powershell
function ConvertTo-CanonicalPath { param([string]$Path) [IO.Path]::GetFullPath($Path).TrimEnd('\') }
function Assert-ContainedPath {
  param([string]$Root,[string]$Candidate)
  $root = (ConvertTo-CanonicalPath $Root) + '\'; $candidate = [IO.Path]::GetFullPath($Candidate)
  if (-not $candidate.StartsWith($root,[StringComparison]::OrdinalIgnoreCase)) { throw "Candidate outside root: $candidate" }
  $candidate
}
function Assert-NoReparsePath {
  param([string]$Path)
  $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop
  if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Reparse point rejected: $Path" }
}
function Assert-SafeTree {
  param([string]$Root,[string]$Candidate)
  $full = Assert-ContainedPath $Root $Candidate; $cursor = ConvertTo-CanonicalPath $Root
  Assert-NoReparsePath $cursor
  $relative = $full.Substring($cursor.Length).TrimStart('\')
  foreach ($part in $relative.Split('\',[StringSplitOptions]::RemoveEmptyEntries)) { $cursor = Join-Path $cursor $part; if (Test-Path -LiteralPath $cursor) { Assert-NoReparsePath $cursor } }
  $full
}
function Assert-RunId { param([string]$RunId) $guid=[guid]::Empty; if (-not [guid]::TryParseExact($RunId,'D',[ref]$guid) -or $guid.ToString('D') -cne $RunId) { throw 'Run ID must be a canonical GUID' }; $RunId }
function Get-Sha256Hex { param([string]$Path) (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function ConvertTo-CanonicalJson { param($Value) [Text.Encoding]::UTF8.GetBytes(($Value | ConvertTo-Json -Depth 32 -Compress)) }
function Write-JsonCreateNew { param([string]$Path,$Value) $bytes=ConvertTo-CanonicalJson $Value; $s=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None); try{$s.Write($bytes,0,$bytes.Length)}finally{$s.Dispose()} }
function Read-JsonFile { param([string]$Path) Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json }
function Get-DeterministicManifest { param([string]$InputRoot,[string]$ScenarioId,[string]$NetworkMode,[string]$SourceRevision)
  $root=ConvertTo-CanonicalPath $InputRoot; Assert-NoReparsePath $root; $seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  $files=Get-ChildItem -LiteralPath $root -File -Recurse -Force | ForEach-Object { Assert-SafeTree $root $_.FullName|Out-Null; $name=$_.FullName.Substring($root.Length).TrimStart('\').Replace('\','/').ToLowerInvariant(); if(-not $seen.Add($name)){throw "Duplicate normalized input: $name"}; [ordered]@{path=$name;length=[int64]$_.Length;sha256=Get-Sha256Hex $_.FullName} } | Sort-Object { $_.path }
  $manifest=[ordered]@{run_independent=$true;scenario_id=$ScenarioId;network_mode=$NetworkMode;source_revision=$SourceRevision;files=@($files)}
  $manifest.manifest_hash=([Security.Cryptography.SHA256]::Create().ComputeHash((ConvertTo-CanonicalJson $manifest))|ForEach-Object ToString x2)-join ''; $manifest
}
```

Implement `Get-DeterministicManifest` by calling `Assert-SafeTree` for each enumerated file, converting its root-relative path to lowercase ordinal `/` form, rejecting duplicate names, sorting ordinally, and hashing the UTF-8 bytes of the canonical ordered JSON object. Export every function above and `Get-DeterministicManifest` with `Export-ModuleMember`.

- [ ] **Step 4: Run the primitive test and verify green.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed
powershell.exe -NoProfile -Command "Import-Module .\scripts\windows-sandbox\lib\Harness.Common.psm1 -Force"
```

Expected: PASS; tests cover prefix siblings, `..`, reparse ancestors, canonical GUID parsing, create-new behavior, ordinal manifest ordering, duplicate normalized names, and stable SHA-256.

- [ ] **Step 5: Commit primitives.**

```powershell
git add scripts/windows-sandbox/lib/Harness.Common.psm1 scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: add sandbox harness safety primitives"
```

### Task 2: Add Exact Schemas And DOM-Safe Profile Templates

**Files:**
- Create: `scripts/windows-sandbox/schemas/scenario.schema.json`
- Create: `scripts/windows-sandbox/schemas/input-manifest.schema.json`
- Create: `scripts/windows-sandbox/schemas/complete.schema.json`
- Create: `scripts/windows-sandbox/templates/agent-manager-sandbox-offline.wsb.xml`
- Create: `scripts/windows-sandbox/templates/agent-manager-sandbox-online.wsb.xml`
- Modify: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`

**Consumes:** shared canonical JSON/XML helpers.
**Produces:** strict descriptor, manifest, and completion contracts plus two templates with empty DOM-populated text fields.

- [ ] **Step 1: Write failing schema/profile assertions.**

```powershell
It 'defines only the two hardened mapped folders' {
  [xml]$xml = Get-Content "$PSScriptRoot/../templates/agent-manager-sandbox-offline.wsb.xml"
  @($xml.Configuration.MappedFolders.MappedFolder).Count | Should -Be 2
  $xml.Configuration.Networking | Should -Be 'Disable'
  foreach($n in 'ClipboardRedirection','PrinterRedirection','AudioInput','VideoInput','VGpu'){ $xml.Configuration.$n | Should -Be 'Disable' }
}
It 'requires identity and hashes in the completion schema' {
  $schema=Get-Content "$PSScriptRoot/../schemas/complete.schema.json" -Raw | ConvertFrom-Json
  @($schema.required) | Should -Contain 'configuration_hash'; @($schema.required) | Should -Contain 'evidence_hashes'
}
```

- [ ] **Step 2: Run the focused test and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`
Expected: FAIL because templates and schemas do not exist.

- [ ] **Step 3: Create strict schemas and valid templates.**

`complete.schema.json` is the following complete contract; `scenario.schema.json` requires `id`, `platform`, `timeout_seconds`, `required_evidence`, and accepts only `online` or `offline` `network_mode`; the launcher supplies offline when absent. `input-manifest.schema.json` requires `run_independent`, `scenario_id`, `network_mode`, `source_revision`, `files`, and `manifest_hash`, with each file requiring `path`, `length`, and `sha256`.

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["run_id","scenario_id","profile","input_manifest_hash","configuration_hash","source_revision","started_at_utc","ended_at_utc","status","evidence_hashes","finalized_at_utc"],"properties":{"run_id":{"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"},"scenario_id":{"type":"string","pattern":"^[a-z0-9-]+$"},"profile":{"enum":["offline","online"]},"input_manifest_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"configuration_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"source_revision":{"type":"string","pattern":"^[0-9a-f]{40}$"},"started_at_utc":{"type":"string","format":"date-time"},"ended_at_utc":{"type":"string","format":"date-time"},"status":{"enum":["complete"]},"evidence_hashes":{"type":"object","minProperties":1,"additionalProperties":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"finalized_at_utc":{"type":"string","format":"date-time"}}}
```

```xml
<Configuration><VGpu>Disable</VGpu><Networking>Disable</Networking><ClipboardRedirection>Disable</ClipboardRedirection><PrinterRedirection>Disable</PrinterRedirection><AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput><MappedFolders><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders><LogonCommand><Command></Command></LogonCommand></Configuration>
```

The online template is identical except `<Networking>Enable</Networking>`. The host never performs XML string replacement: it loads `[xml]`, assigns both `HostFolder` `InnerText` values and the one `Command` `InnerText`, saves, reloads, and validates every node against the exact policy.

- [ ] **Step 4: Run profile/schema assertions and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`
Expected: PASS; test rejects a third map, mutable input, wrong sandbox path, enabled redirected device, template command injection, malformed hash, absent completion identity, and invalid network mode.

- [ ] **Step 5: Commit schemas and templates.**

```powershell
git add scripts/windows-sandbox/schemas scripts/windows-sandbox/templates scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: define sandbox configuration contracts"
```

### Task 3: Declare The Exact 28-Scenario Boundary

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

**Consumes:** scenario schema and authoritative matrix.
**Produces:** ten profile-routed Windows descriptors and eighteen non-interactive macOS records.

- [ ] **Step 1: Write the failing matrix test.**

```powershell
$windows=Get-ChildItem "$PSScriptRoot/../scenarios/windows" -Filter *.json | ForEach-Object { Get-Content $_ -Raw|ConvertFrom-Json }
$macos=(Get-Content "$PSScriptRoot/../scenarios/macos/matrix.json" -Raw|ConvertFrom-Json).scenarios
@($windows).Count|Should -Be 10; @($macos|Where-Object architecture -eq intel).Count|Should -Be 9
@($macos|Where-Object architecture -eq apple_silicon).Count|Should -Be 9
@($macos|Where-Object { $_.execution_status -ne 'blocked' -or $_.evidence_kind -ne 'static_only' }).Count|Should -Be 0
```

- [ ] **Step 2: Run the matrix test and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`
Expected: FAIL because descriptors and macOS matrix do not exist.

- [ ] **Step 3: Create the descriptors.**

Each Windows file uses the same complete shape below with its filename as `id`, a positive bounded timeout, declared fixture process, and required records. Set `proxy-failure` to `online`; set the remaining nine listed Windows IDs to `offline`. The macOS matrix lists the nine authoritative Intel and nine Apple Silicon cases with `execution_status: "blocked"`, `evidence_kind: "static_only"`, and `requires_disposable_macos: true`.

```json
{"id":"proxy-failure","platform":"windows","network_mode":"online","timeout_seconds":180,"declared_process":"fixture-proxy-failure.ps1","required_evidence":["started.json","provenance.json","results.tsv","sandbox-transcript.txt"]}
```

- [ ] **Step 4: Run the matrix test and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`
Expected: PASS; a descriptor without declared profile or a macOS record marked interactive fails.

- [ ] **Step 5: Commit the matrix.**

```powershell
git add scripts/windows-sandbox/scenarios scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "feat: declare sandbox scenario matrix"
```

### Task 4: Stage Inputs And Create A Fresh Host-Controlled Run

**Files:**
- Create: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/input/runner.txt`
- Create: `scripts/windows-sandbox/tests/fixtures/input/artifact.txt`
- Create: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** tracked source revision, descriptor, external runtime root, and primitive module.
**Produces:** staged `test\input`, matching input/control manifests, UUID run container, and create-new `control\launch.json`.

- [ ] **Step 1: Write failing staging tests.**

```powershell
It 'refuses an existing run container without deleting it' {
  $id='00000000-0000-0000-0000-000000000001'; $run=Join-Path $TestDrive "evidence\$id"; New-Item -ItemType Directory -Force $run|Out-Null
  { New-HarnessRun -EvidenceRoot (Join-Path $TestDrive 'evidence') -RunId $id } | Should -Throw '*exists*'; Test-Path $run|Should -BeTrue
}
It 'refuses input content not listed by the canonical manifest' {
  $input=Join-Path $TestDrive 'input'; New-Item -ItemType Directory -Path $input|Out-Null; [IO.File]::WriteAllText((Join-Path $input 'runner.txt'),'ok')
  $manifest=Get-DeterministicManifest -InputRoot $input -ScenarioId path-refresh -NetworkMode offline -SourceRevision ('a'*40); Write-JsonCreateNew (Join-Path $input 'input-manifest.json') $manifest; [IO.File]::WriteAllText((Join-Path $input 'extra.bin'),'x')
  { Assert-StagedInput -InputRoot $input } | Should -Throw '*unexpected*'
}
```

- [ ] **Step 2: Run staging tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: FAIL because launcher staging functions are absent.

- [ ] **Step 3: Implement strict staging lifecycle.**

```powershell
function New-HarnessRun {
 param([string]$EvidenceRoot,[string]$RunId)
 Assert-RunId $RunId; Assert-SafeTree (Split-Path $EvidenceRoot -Parent) $EvidenceRoot
 $run=Join-Path $EvidenceRoot $RunId; if(Test-Path -LiteralPath $run){throw "Run container exists: $run"}
 [IO.Directory]::CreateDirectory((Join-Path $run 'control'))|Out-Null; [IO.Directory]::CreateDirectory((Join-Path $run 'sandbox-output'))|Out-Null
 if(@(Get-ChildItem -LiteralPath (Join-Path $run 'sandbox-output') -Force).Count){throw 'sandbox-output is not empty'}
 [pscustomobject]@{RunId=$RunId;RunRoot=$run;ControlRoot=(Join-Path $run 'control');OutputRoot=(Join-Path $run 'sandbox-output')}
}
function Assert-StagedInput { param([string]$InputRoot)
  $manifest=Read-JsonFile (Join-Path $InputRoot 'input-manifest.json'); $expected=@($manifest.files|ForEach-Object path); $actual=Get-ChildItem -LiteralPath $InputRoot -File -Recurse -Force|ForEach-Object {$_.FullName.Substring((ConvertTo-CanonicalPath $InputRoot).Length).TrimStart('\').Replace('\','/').ToLowerInvariant()}|Where-Object {$_ -ne 'input-manifest.json'}
  foreach($name in $actual){if($expected -notcontains $name){throw "Unexpected staged input: $name"}}; foreach($entry in $manifest.files){$path=Assert-SafeTree $InputRoot (Join-Path $InputRoot $entry.path.Replace('/','\'));if(-not(Test-Path $path)){throw "Missing staged input: $($entry.path)"};if((Get-Sha256Hex $path) -cne $entry.sha256){throw "Staged hash mismatch: $($entry.path)"}}
}
```

Copy the selected tracked revision to a newly created sibling staging directory only after `Assert-SafeTree` rejects reparse points; create and validate the deterministic manifest; write it using `Write-JsonCreateNew` to staging and `control`; then rename staging to external `test\input` only when the dedicated prior input root is absent or a verified dedicated managed root. If replacement cannot be performed without delete/overwrite ambiguity, throw and retain the existing input root. The staged allowlist excludes all evidence, external legacy WSB/PS1 files, and unrelated test-root files.

- [ ] **Step 4: Run staging tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: PASS; duplicate/malformed IDs, stale output, reparse input, extra/missing/hash-mismatched files, prior control collision, and non-atomic staging fail before launch.

- [ ] **Step 5: Commit host staging.**

```powershell
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/fixtures/input scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: stage sandbox inputs safely"
```

### Task 5: Generate Matched WSB Profiles And Bounded Launch Records

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** fresh run object, descriptor, manifest hash, templates, and source revision.
**Produces:** validated profile WSB plus immutable `control\launch.json` before the single Sandbox process starts.

- [ ] **Step 1: Write failing mapping/mismatch tests.**

```powershell
It 'refuses profile disagreement before Start-Process' {
  $run=[pscustomobject]@{RunId='00000000-0000-0000-0000-000000000001';ControlRoot='C:\t\evidence\r\control';OutputRoot='C:\t\evidence\r\sandbox-output'}
  { New-Configuration -Scenario ([pscustomobject]@{id='path-refresh';network_mode='offline'}) -Profile online -Run $run -InputRoot 'C:\t\input' } | Should -Throw '*does not match*'
}
It 'rejects child and parent mappings' {
  { Assert-NonOverlappingMappings -InputRoot 'C:\t\input' -OutputRoot 'C:\t\input\out' -ControlRoot 'C:\t\evidence\r\control' } | Should -Throw '*overlap*'
}
```

- [ ] **Step 2: Run profile tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: FAIL because `New-Configuration` and mapping validation are absent.

- [ ] **Step 3: Implement DOM generation and launch record.**

```powershell
function Assert-NonOverlappingMappings { param([string]$InputRoot,[string]$OutputRoot,[string]$ControlRoot)
 $all=@($InputRoot,$OutputRoot,$ControlRoot)|ForEach-Object{(ConvertTo-CanonicalPath $_)+'\'}
 for($i=0;$i -lt $all.Count;$i++){for($j=$i+1;$j -lt $all.Count;$j++){if($all[$i].StartsWith($all[$j],[StringComparison]::OrdinalIgnoreCase)-or $all[$j].StartsWith($all[$i],[StringComparison]::OrdinalIgnoreCase)){throw 'Mapping overlap'}}}
}
function New-Configuration { param($Scenario,[ValidateSet('offline','online')][string]$Profile,$Run,[string]$InputRoot)
 $actual=if($Scenario.network_mode -eq 'online'){'online'}else{'offline'}; if($Profile -ne $actual){throw 'Requested profile does not match descriptor'}
 Assert-NonOverlappingMappings $InputRoot $Run.OutputRoot $Run.ControlRoot
 [xml]$xml=Get-Content (Join-Path $PSScriptRoot "templates\agent-manager-sandbox-$Profile.wsb.xml") -Raw
 $maps=@($xml.Configuration.MappedFolders.MappedFolder); $maps[0].HostFolder=$InputRoot; $maps[1].HostFolder=$Run.OutputRoot
 $xml.Configuration.LogonCommand.Command="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\AgentManagerHarness\Input\Invoke-SandboxRunner.ps1 -ScenarioPath C:\AgentManagerHarness\Input\scenario.json -RunId $($Run.RunId) -Profile $Profile"
 $path=Join-Path $Run.ControlRoot "agent-manager-sandbox-$Profile.wsb"; $xml.Save($path); [xml]$check=Get-Content $path -Raw; if(@($check.Configuration.MappedFolders.MappedFolder).Count -ne 2){throw 'Invalid mapping count'}; $path
}
```

Create `launch.json` with `Write-JsonCreateNew` before `Start-Process -FilePath $wsbPath -PassThru`; it contains run ID, scenario ID, profile, input manifest hash, configuration hash, source revision, launcher version, UTC start, explicit output relative name `sandbox-output`, and launched PID. The launcher may stop only that recorded PID after its bounded deadline.

- [ ] **Step 4: Run profile tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: PASS; tests prove the exact two maps, device policy, profile refusal, unmapped control/run roots, DOM-escaped values, and no process start on validation failure.

- [ ] **Step 5: Commit profile generation.**

```powershell
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: generate validated sandbox profiles"
```

### Task 6: Implement Runner Preflight, Owned Process Trees, And Complete-Last Evidence

**Files:**
- Create: `scripts/windows-sandbox/Invoke-SandboxRunner.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/evidence/required.txt`
- Create: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`

**Consumes:** read-only staged manifest/descriptor, run ID/profile, and the one output mapping.
**Produces:** create-new evidence records and `complete.json` only after exact required records/fresh hashes exist.

- [ ] **Step 1: Write failing ownership and completion tests.**

```powershell
It 'kills only the recorded process tree and reports a cleanup failure' {
  $calls=[Collections.Generic.List[int]]::new(); $api=@{GetChildren={param($id) if($id -eq 11){@(12)}else{@()}}; Stop={param($id) if($id -eq 12){throw 'access denied'};$calls.Add($id)}}
  $result=Stop-OwnedProcessTree -RootProcessId 11 -Api $api
  $calls | Should -Contain 11; $calls | Should -Not -Contain 99; $result.cleanup_status | Should -Be 'failed'; $result.failed_process_ids | Should -Contain 12
}
It 'writes no completion marker when required evidence is absent' {
  $identity=[pscustomobject]@{run_id='00000000-0000-0000-0000-000000000001';scenario_id='path-refresh';profile='offline';input_manifest_hash=('a'*64);configuration_hash=('b'*64);source_revision=('c'*40);started_at_utc='2026-07-14T00:00:00.0000000Z'}
  $r=Finalize-Evidence -EvidenceRoot $TestDrive -RequiredEvidence @('results.tsv','screen.png') -Identity $identity
  $r.status | Should -Be 'blocked'; Test-Path (Join-Path $TestDrive 'complete.json') | Should -BeFalse
}
```

- [ ] **Step 2: Run runner tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed`
Expected: FAIL because runner functions do not exist.

- [ ] **Step 3: Implement runner validation, ownership, and finalization.**

`Assert-RunnerPreflight` calls `Assert-RunId`, re-hashes every manifest entry at `C:\AgentManagerHarness\Input`, rejects extra/missing input, and checks descriptor/profile before any declared process starts. It writes a minimal create-new `failure.json` if output is usable and exits nonzero; extraction is restricted to `%TEMP%\AgentManagerSandbox\$RunId`.

```powershell
function Stop-OwnedProcessTree { param([int]$RootProcessId,[hashtable]$Api)
 $queue=[Collections.Generic.Queue[int]]::new(); $seen=[Collections.Generic.HashSet[int]]::new(); $queue.Enqueue($RootProcessId); $ordered=[Collections.Generic.List[int]]::new()
 while($queue.Count){$id=$queue.Dequeue();if($seen.Add($id)){foreach($child in & $Api['GetChildren'] $id){$queue.Enqueue([int]$child)};$ordered.Add($id)}}
 $failed=[Collections.Generic.List[int]]::new(); foreach($id in @($ordered|Sort-Object -Descending)){try{& $Api['Stop'] $id}catch{$failed.Add($id)}}
 [pscustomobject]@{cleanup_status=if($failed.Count){'failed'}else{'succeeded'};failed_process_ids=@($failed);owned_process_ids=@($ordered)}
}
function Finalize-Evidence { param([string]$EvidenceRoot,[string[]]$RequiredEvidence,$Identity)
 $hashes=[ordered]@{}; foreach($name in $RequiredEvidence){$path=Assert-SafeTree $EvidenceRoot (Join-Path $EvidenceRoot $name);if(-not(Test-Path $path)-or (Get-Item $path).Length -eq 0){return [pscustomobject]@{status='blocked'}};$hashes[$name]=Get-Sha256Hex $path}
 $complete=[ordered]@{run_id=$Identity.run_id;scenario_id=$Identity.scenario_id;profile=$Identity.profile;input_manifest_hash=$Identity.input_manifest_hash;configuration_hash=$Identity.configuration_hash;source_revision=$Identity.source_revision;started_at_utc=$Identity.started_at_utc;ended_at_utc=(Get-Date).ToUniversalTime().ToString('o');status='complete';evidence_hashes=$hashes;finalized_at_utc=(Get-Date).ToUniversalTime().ToString('o')}
 Write-JsonCreateNew (Join-Path $EvidenceRoot 'complete.json') $complete; [pscustomobject]@{status='complete';complete=$complete}
}
```

Start only `declared_process` using `Start-Process -PassThru`; query its descendants by parent PID, apply `timeout_seconds`, and call `Stop-OwnedProcessTree` only with that returned root PID. Record a cleanup failure in a create-new record, return failure/blocked, and never terminate by image name or product-name pattern. Remove only the validated sandbox-local temporary directory in `finally` after finalization attempt.

- [ ] **Step 4: Run runner tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed`
Expected: PASS; test proves spawned descendant IDs are stopped, unrelated PID 99 is untouched, failed descendant stop is recorded, no `complete.json` follows failure, collisions throw, and complete marker includes every schema-required identity/timestamp/hash field.

- [ ] **Step 5: Commit runner evidence.**

```powershell
git add scripts/windows-sandbox/Invoke-SandboxRunner.ps1 scripts/windows-sandbox/tests/fixtures/evidence scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1
git commit -m "feat: finalize sandbox evidence safely"
```

### Task 7: Collect, Validate, And Archive By Explicit Canonical Run ID

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** explicit canonical run ID returned by launcher, matching unmapped `control\launch.json`, `control\input-manifest.json`, generated WSB, and that run's `sandbox-output`.
**Produces:** create-new `control\collection.json` and optional create-new `control\archive.json`, never a sibling scan or mutable archive mapping.

- [ ] **Step 1: Write failing collector and archive tests.**

```powershell
It 'rejects a marker that differs from unmapped control records' {
  $launch=[pscustomobject]@{run_id='00000000-0000-0000-0000-000000000001';scenario_id='path-refresh';profile='offline';input_manifest_hash=('a'*64);configuration_hash=('b'*64);source_revision=('c'*40)}
  $complete=[pscustomobject]@{run_id=$launch.run_id;scenario_id=$launch.scenario_id;profile=$launch.profile;input_manifest_hash=('d'*64);configuration_hash=$launch.configuration_hash;source_revision=$launch.source_revision;status='complete'}
  { Assert-CollectedEvidence -Launch $launch -Complete $complete } | Should -Throw '*input_manifest_hash*'
}
It 'refuses an archive path outside the same control directory' {
  { New-ArchiveRecord -ControlRoot 'C:\evidence\r\control' -ArchivePath 'C:\evidence\other\archive.json' -RunId '00000000-0000-0000-0000-000000000001' } | Should -Throw '*outside root*'
}
```

- [ ] **Step 2: Run collector tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: FAIL because collector functions do not exist.

- [ ] **Step 3: Implement canonical collection and archive record.**

```powershell
function Assert-CollectedEvidence { param($Launch,$Complete)
 foreach($name in 'run_id','scenario_id','profile','input_manifest_hash','configuration_hash','source_revision'){if($Launch.$name -cne $Complete.$name){throw "Evidence mismatch: $name"}}
 if($Complete.status -ne 'complete'){throw 'Evidence is not complete'}
 foreach($entry in $Complete.evidence_hashes.psobject.Properties){if($entry.Value -notmatch '^[0-9a-f]{64}$'){throw "Invalid evidence hash: $($entry.Name)"}}
}
function Get-RunControlRecord { param([string]$RunId,[string]$EvidenceRoot)
 Assert-RunId $RunId; $run=Assert-SafeTree $EvidenceRoot (Join-Path $EvidenceRoot $RunId); $control=Assert-SafeTree $run (Join-Path $run 'control'); $launch=Read-JsonFile (Join-Path $control 'launch.json')
 if($launch.run_id -cne $RunId -or $launch.output_relative_name -cne 'sandbox-output'){throw 'Invalid canonical launch record'}
 [pscustomobject]@{run_id=$RunId;control_root=$control;output_root=(Assert-SafeTree $run (Join-Path $run 'sandbox-output'));launch=$launch}
}
function New-ArchiveRecord { param([string]$ControlRoot,[string]$ArchivePath,[string]$RunId)
 Assert-RunId $RunId; $target=Assert-SafeTree $ControlRoot $ArchivePath
 Write-JsonCreateNew $target ([ordered]@{run_id=$RunId;created_at_utc=(Get-Date).ToUniversalTime().ToString('o');kind='validated-evidence-index'})
}
```

The collector derives `OutputRoot` solely as `Join-Path $Run.ControlRoot '..\sandbox-output'` after canonical containment and validates the path is exactly the run container's output child; it does not enumerate `test\evidence`. It loads `control\launch.json` with `Read-JsonFile`, compares all completion fields and each current output file hash to `complete.json`, applies a documented 120-second UTC skew allowance, and writes `collection.json` create-new with accepted/failed/blocked result. Archiving is a control-only index record created as `control\archive.json` after validation; it contains the explicit run ID, canonical manifest/config hashes, and accepted file hashes, does not copy/extract files, and cannot traverse or overwrite.

- [ ] **Step 4: Run collector tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`
Expected: PASS; wrong identity/hash/revision/timestamp, malformed final marker, missing fresh output, sibling output, archive traversal, archive collision, Sandbox timeout, and cleanup failure remain failed or blocked with partial evidence retained.

- [ ] **Step 5: Commit collector policy.**

```powershell
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: collect sandbox evidence by run id"
```

### Task 8: Document Fail-Closed Migration, Rollback, And Future Execution

**Files:**
- Create: `scripts/windows-sandbox/README.md`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Modify: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** versioned harness, external legacy file locations, explicit run ID, and accepted control records.
**Produces:** executable future-only protocol that inventories and retires unsafe legacy files without launching them and never restores their broad mapping.

- [ ] **Step 1: Write failing documentation-contract tests.**

```powershell
It 'documents read-only legacy inventory and no unsafe fallback' {
 $text=Get-Content "$PSScriptRoot/../README.md" -Raw
 $text|Should -Match 'Get-Content -LiteralPath D:\\codex\\ai-deploy-toolkit\\test\\agent-manager-test.wsb'
 $text|Should -Match 'never launch the legacy files'
 $text|Should -Match 'never restore.*writable test-root mapping'
}
```

- [ ] **Step 2: Run documentation tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`
Expected: FAIL because the runtime protocol does not exist.

- [ ] **Step 3: Write the concrete protocol and future-only commands.**

The README states that migration reads, but never launches, `D:\codex\ai-deploy-toolkit\test\agent-manager-test.wsb` and `D:\codex\ai-deploy-toolkit\test\run-agent-manager-sandbox.ps1` with `Get-Content -LiteralPath`; inventory is saved through `Write-JsonCreateNew` in a new run's unmapped control directory. After static Pester preflight passes, future implementation can archive legacy files to a timestamped host location using an explicit operator-approved source/destination and only after `Assert-SafeTree` and reparse validation; it must fail if the destination exists, never delete the source, and never launch either legacy file. Rollback selects a prior validated tracked `scripts\windows-sandbox` Git revision in an isolated checkout, stages it into a new input/run ID, and proves a complete offline record without restoring the old writable test-root map.

Add this host-only function to `Invoke-AgentManagerSandbox.ps1`; it is invoked only after Pester preflight and a new run's `control\legacy-inventory.json` exists, never by a Sandbox logon command:

```powershell
function Move-LegacyHarnessToQuarantine {
  param([string]$TestRoot,[string]$ControlRoot,[string]$RunId)
  Assert-RunId $RunId; $wsb=Assert-SafeTree $TestRoot (Join-Path $TestRoot 'agent-manager-test.wsb'); $runner=Assert-SafeTree $TestRoot (Join-Path $TestRoot 'run-agent-manager-sandbox.ps1')
  $inventory=[ordered]@{run_id=$RunId;read_at_utc=(Get-Date).ToUniversalTime().ToString('o');files=@([ordered]@{path=$wsb;sha256=Get-Sha256Hex $wsb},[ordered]@{path=$runner;sha256=Get-Sha256Hex $runner})}
  Write-JsonCreateNew (Join-Path $ControlRoot 'legacy-inventory.json') $inventory
  $retired=Join-Path $TestRoot (Join-Path 'retired' $RunId); [IO.Directory]::CreateDirectory($retired)|Out-Null
  foreach($source in @($wsb,$runner)){Assert-NoReparsePath $source; $target=Assert-SafeTree $retired (Join-Path $retired (Split-Path $source -Leaf)); if(Test-Path -LiteralPath $target){throw "Legacy target exists: $target"}; Move-Item -LiteralPath $source -Destination $target -ErrorAction Stop}
}
```

The README must direct operators to stop immediately if inventory or either move fails, preserve the source/partial quarantine state for review, and use neither archived file as an execution fallback.

Add the following commands verbatim under **FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION**. Each command uses the exact worktree and each descriptor's declared profile:

```powershell
Set-Location D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId clean-install -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId uac-accept -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId uac-decline -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId path-refresh -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId multiple-node-installations -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId proxy-failure -Profile online -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId file-lock -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId disk-space-guard -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId batch-partial-failure -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId postflight-path-version -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test
```

For a returned launcher run ID, use only `Get-RunControlRecord -RunId $runId -EvidenceRoot D:\codex\ai-deploy-toolkit\test\evidence` and `New-ArchiveRecord -ControlRoot $record.control_root -ArchivePath (Join-Path $record.control_root 'archive.json') -RunId $runId`; do not choose newest output, enumerate sibling evidence, extract ZIP files, or create writable archive mappings.

- [ ] **Step 4: Run documentation tests and all static tests.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests -Output Detailed
git diff --check
```

Expected: PASS; Pester uses only fixtures/TestDrive and launches no Sandbox, MSI, EXE, WebView installer, or product process.

- [ ] **Step 5: Commit operational documentation.**

```powershell
git add scripts/windows-sandbox/README.md scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1 scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "docs: document hardened sandbox migration"
```

### Task 9: Future-Only Disposable Acceptance

**Files:**
- Create: `docs/superpowers/verification/2026-07-14-windows-sandbox-harness-hardening.md`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** implementation commits, explicit launcher run IDs, canonical control records, disposable Windows Sandbox, and separate disposable macOS Intel/Apple Silicon environments.
**Produces:** honest Windows interaction evidence and a separately blocked or observed macOS report.

- [ ] **Step 1: Run static preflight before future execution.**

Run from `D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator`:

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests -Output Detailed
git diff --check
```

Expected: PASS before any launch; record the exact harness commit and Pester result in the verification document.

- [ ] **Step 2: Execute the ten Windows descriptors only in a disposable Sandbox.**

Run: the ten explicitly listed **FUTURE EXECUTION ONLY** commands in Task 8, once each, with their declared profile.
Expected: every accepted run returns its explicit canonical run ID, retains unchanged control/input boundaries, changes only its named `sandbox-output`, and has matching input-manifest/configuration/source hashes, declared evidence, and final completion record.

- [ ] **Step 3: Validate and archive every returned run by its explicit ID.**

Run: `Get-RunControlRecord -RunId $runId -EvidenceRoot D:\codex\ai-deploy-toolkit\test\evidence` followed by `New-ArchiveRecord` exactly as Task 8 specifies for each returned ID.
Expected: control and output records match; crash, timeout, cleanup failure, timestamp violation, missing file, stale hash, or absent completion marker is failed/blocked and preserved rather than accepted.

- [ ] **Step 4: Record macOS evidence without static equivalence.**

Run each of the nine Intel and nine Apple Silicon scenarios only on the respective real disposable macOS environment.
Expected: a platform case is accepted only with native interaction logs/screenshots and recorded OS/architecture; unavailable hardware remains `BLOCKED`, while Windows, static, hash, ZIP, or Mach-O results never change macOS acceptance.

- [ ] **Step 5: Commit the future verification record.**

```powershell
git add docs/superpowers/verification/2026-07-14-windows-sandbox-harness-hardening.md
git commit -m "docs: verify sandbox harness hardening"
```

## Acceptance Criteria And Self-Review

- Tasks 1-2 define all named helpers, canonical containment/reparse traversal, deterministic serialization/hash, strict GUID parsing, create-new records, exact schemas, and DOM-safe WSB policy.
- Tasks 3-5 encode all ten Windows and eighteen macOS cases, external immutable staging, fresh unmapped control/output ownership, profile mismatch refusal, and exact mapping validation.
- Tasks 6-7 prove manifest revalidation, explicit owned process-tree cleanup, cleanup failure recording, complete-last evidence, canonical collector comparisons, no sibling scan, and create-new control archive indexing.
- Tasks 8-9 provide read-only legacy inventory, fail-closed retirement, isolated tracked rollback, exact future Windows commands, and real macOS-only interactive acceptance.
- Review scans must reject unresolved planning markers, ellipses, wrong worktree roots, undefined helper names, broad filesystem erasure, image-name process termination, sibling-run selection, archive extraction, and wording that converts macOS static evidence into interaction evidence. Reconcile every Files/Consumes/Produces declaration, function signature, schema field, descriptor ID/profile, command, expected red/green result, and commit path before execution.

Plan complete and saved to `docs/superpowers/plans/2026-07-14-windows-sandbox-harness-hardening.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task and review each task before the next.
2. Inline Execution - execute tasks in this session using executing-plans with review checkpoints.
