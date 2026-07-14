# Windows Sandbox Harness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a versioned, fail-closed Windows Sandbox harness that stages hash-validated immutable inputs and accepts only complete evidence from one exclusively claimed, per-run writable output directory.

**Architecture:** Preserve approved Alternative B: tracked source under `scripts/windows-sandbox` is staged into one external read-only input mapping, while each GUID run owns an unmapped `control` child and one fresh writable `sandbox-output` child. A create-new claim file exclusively owns the run before children exist; immutable intent/process records resolve launch chronology, and both host and Sandbox use repository-owned strict validators plus identical canonical manifest bytes.

**Tech Stack:** Windows PowerShell 5.1, Pester 5, Windows Sandbox XML, SHA-256, Git; no runtime package installation and no unprovided JSON Schema engine.

**Global Constraints:**

- Run every repository command from `D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator`; never operate from another checkout.
- `D:\codex\ai-deploy-toolkit\test` is external runtime data. Only `scripts\windows-sandbox` is tracked source; never map or commit the external test-root parent.
- A run ID is a newly generated lowercase GUID in canonical `D` form. The only writable Sandbox mapping is that run's fresh `sandbox-output` child.
- Offline is default. Only descriptor `network_mode: "online"` permits the online template, and caller/profile disagreement fails before launch.
- Both profiles disable clipboard, printer, audio input, video input, and vGPU. They contain exactly two maps: `test\input` read-only at `C:\AgentManagerHarness\Input`, and `sandbox-output` read-write at `C:\AgentManagerHarness\Evidence`.
- Reject reparse points, traversal, prefix siblings, duplicate normalized names, unlisted files, hash mismatch, stale output, collisions, malformed records, cleanup failure, and timeouts. Never kill by image name, overwrite evidence, enumerate sibling runs, or delete a runtime path to retry.
- The truth boundary is exactly 10 Windows scenarios plus 9 macOS Intel and 9 macOS Apple Silicon scenarios. Static macOS checks are never interactive acceptance; unavailable real disposable hardware remains blocked.
- All implementation tests use `TestDrive` and harmless fixtures. Sandbox, installers, product binaries, MSI, EXE, and external-test commands are future-only and explicitly gated.

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

### Task 1: Implement Canonical Paths, Exclusive Writes, And Manifest Bytes

**Files:**
- Create: `scripts/windows-sandbox/lib/Harness.Common.psm1`
- Create: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`

**Consumes:** Windows PowerShell 5.1 filesystem and SHA-256 APIs.
**Produces:** canonical containment/reparse guards, create-new byte/JSON writers, ordinal sorting, and one manifest-byte algorithm used on host and in Sandbox.

- [ ] **Step 1: Write failing primitive and byte-identity tests.**

```powershell
Import-Module "$PSScriptRoot/../lib/Harness.Common.psm1" -Force
Describe 'canonical safety primitives' {
  It 'rejects prefix siblings and reparse ancestors' {
    { Assert-ContainedPath -Root 'C:\work\input' -Candidate 'C:\work\input-old\a' } | Should -Throw '*outside root*'
  }
  It 'uses create-new without changing an existing record' {
    $p=Join-Path $TestDrive 'record.json'; [IO.File]::WriteAllText($p,'{"old":true}',[Text.UTF8Encoding]::new($false))
    { Write-JsonCreateNew -Path $p -Value ([ordered]@{run_id='00000000-0000-0000-0000-000000000001'}) } | Should -Throw '*exists*'
    [IO.File]::ReadAllText($p) | Should -Be '{"old":true}'
  }
  It 'sorts ordinally independent of current culture and hashes the payload without manifest_hash' {
    $old=[Globalization.CultureInfo]::CurrentCulture
    try { [Globalization.CultureInfo]::CurrentCulture=[Globalization.CultureInfo]::GetCultureInfo('tr-TR'); $m=Get-DeterministicManifest -InputRoot $fixture -ScenarioId clean-install -NetworkMode offline -SourceRevision ('a'*40); $m.files.path | Should -Be @('i.txt','z.txt'); (Get-ManifestPayloadBytes $m) | Should -Be (Get-ManifestPayloadBytes (Read-JsonFile $saved)) } finally { [Globalization.CultureInfo]::CurrentCulture=$old }
  }
}
```

- [ ] **Step 2: Run the focused test and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`

Expected: FAIL because the module and functions do not exist.

- [ ] **Step 3: Implement PowerShell 5.1-safe primitives and the exact manifest algorithm.**

```powershell
function ConvertTo-CanonicalPath { param([string]$Path) [IO.Path]::GetFullPath($Path).TrimEnd('\') }
function Assert-ContainedPath {
  param([string]$Root,[string]$Candidate,[switch]$AllowRoot)
  $r=ConvertTo-CanonicalPath $Root; $c=[IO.Path]::GetFullPath($Candidate)
  if(($AllowRoot -and $c.Equals($r,[StringComparison]::OrdinalIgnoreCase)) -or $c.StartsWith($r+'\',[StringComparison]::OrdinalIgnoreCase)){return $c}
  throw "Candidate outside root: $c"
}
function Assert-NoReparsePath { param([string]$Path) $i=Get-Item -LiteralPath $Path -Force -ErrorAction Stop; if(($i.Attributes -band [IO.FileAttributes]::ReparsePoint)-ne 0){throw "Reparse point rejected: $Path"} }
function Assert-SafeTree {
  param([string]$Root,[string]$Candidate,[switch]$AllowRoot)
  $r=ConvertTo-CanonicalPath $Root; $c=Assert-ContainedPath $r $Candidate -AllowRoot:$AllowRoot; Assert-NoReparsePath $r
  $cursor=$r; foreach($part in $c.Substring($r.Length).TrimStart('\').Split(@('\'),[StringSplitOptions]::RemoveEmptyEntries)){ $cursor=Join-Path $cursor $part; if(Test-Path -LiteralPath $cursor){Assert-NoReparsePath $cursor} }; $c
}
function Assert-RunId { param([string]$RunId) $g=[guid]::Empty; if(-not [guid]::TryParseExact($RunId,'D',[ref]$g)-or $g.ToString('D')-cne $RunId){throw 'Run ID must be a canonical lowercase GUID'}; $RunId }
function Get-Sha256Bytes { param([byte[]]$Bytes) $sha=[Security.Cryptography.SHA256]::Create(); try{(($sha.ComputeHash($Bytes)|ForEach-Object {$_.ToString('x2')})-join '')}finally{$sha.Dispose()} }
function Get-Sha256Hex { param([string]$Path) (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function ConvertTo-CanonicalJsonBytes { param($Value) [Text.UTF8Encoding]::new($false).GetBytes(($Value|ConvertTo-Json -Depth 32 -Compress)) }
function Write-BytesCreateNew { param([string]$Path,[byte[]]$Bytes) try{$s=[IO.File]::Open($Path,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)}catch[IO.IOException]{throw "Path exists or cannot be exclusively created: $Path"}; try{$s.Write($Bytes,0,$Bytes.Length);$s.Flush()}finally{$s.Dispose()} }
function Write-JsonCreateNew { param([string]$Path,$Value) Write-BytesCreateNew $Path (ConvertTo-CanonicalJsonBytes $Value) }
function Read-JsonFile { param([string]$Path) [IO.File]::ReadAllText($Path,[Text.Encoding]::UTF8)|ConvertFrom-Json }
function Sort-OrdinalStrings { param([string[]]$Values) $copy=[string[]]@($Values); [Array]::Sort($copy,[StringComparer]::Ordinal); $copy }
function Get-ManifestPayloadObject { param($Manifest) [ordered]@{schema_version=$Manifest.schema_version;scenario_id=$Manifest.scenario_id;network_mode=$Manifest.network_mode;source_revision=$Manifest.source_revision;files=@($Manifest.files|ForEach-Object{[ordered]@{path=$_.path;length=[int64]$_.length;sha256=$_.sha256}})} }
function Get-ManifestPayloadBytes { param($Manifest) ConvertTo-CanonicalJsonBytes (Get-ManifestPayloadObject $Manifest) }
function Get-DeterministicManifest {
  param([string]$InputRoot,[string]$ScenarioId,[string]$NetworkMode,[string]$SourceRevision)
  $root=ConvertTo-CanonicalPath $InputRoot; Assert-SafeTree $root $root -AllowRoot|Out-Null; $byName=@{}; $names=[Collections.Generic.List[string]]::new()
  foreach($file in Get-ChildItem -LiteralPath $root -File -Recurse -Force){Assert-SafeTree $root $file.FullName|Out-Null;$name=$file.FullName.Substring($root.Length).TrimStart('\').Replace('\','/').ToLowerInvariant();if($byName.ContainsKey($name)){throw "Duplicate normalized input: $name"};$byName[$name]=[ordered]@{path=$name;length=[int64]$file.Length;sha256=Get-Sha256Hex $file.FullName};$names.Add($name)}
  $m=[ordered]@{schema_version=1;scenario_id=$ScenarioId;network_mode=$NetworkMode;source_revision=$SourceRevision;files=@(Sort-OrdinalStrings $names|ForEach-Object{$byName[$_]})}; $m.manifest_hash=Get-Sha256Bytes (Get-ManifestPayloadBytes $m); $m
}
```

The hashed bytes are exactly UTF-8 without BOM of compressed ordered JSON containing `schema_version`, `scenario_id`, `network_mode`, `source_revision`, and `files`, in that order. `manifest_hash` is excluded from those bytes, then appended as the final top-level property; validation reconstructs the payload object and bytes, never hashes the serialized on-disk object wholesale.

- [ ] **Step 4: Run tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`

Expected: PASS for prefix siblings, root equality, reparse ancestors, canonical GUIDs, create-new collisions, Turkish-culture ordering, duplicate normalized names, and host/Sandbox payload-byte equality.

- [ ] **Step 5: Commit primitives.**

```powershell
git add scripts/windows-sandbox/lib/Harness.Common.psm1 scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: add sandbox harness safety primitives"
```

### Task 2: Define Complete Contracts And Repository-Owned Validators

**Files:**
- Create: `scripts/windows-sandbox/schemas/scenario.schema.json`
- Create: `scripts/windows-sandbox/schemas/input-manifest.schema.json`
- Create: `scripts/windows-sandbox/schemas/complete.schema.json`
- Modify: `scripts/windows-sandbox/lib/Harness.Common.psm1`
- Modify: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`

**Consumes:** canonical JSON values and the manifest payload-byte algorithm.
**Produces:** documentation schemas and explicit strict validators; the validators, not PowerShell itself, are runtime enforcement.

- [ ] **Step 1: Write table-driven failing contract tests.**

```powershell
$valid=[ordered]@{schema_version=1;id='clean-install';platform='windows';network_mode='offline';timeout_seconds=180;declared_process='tests/fixtures/input/fixture-clean-install.ps1';arguments=@('-Mode','safe');required_evidence=@('results.tsv','sandbox-transcript.txt')}
It 'rejects unknown, absent, wrong-type, pattern, array, and bound violations' {
  $cases=@(
    { $x=[ordered]@{}+$valid;$x.extra=1;$x },
    { $x=[ordered]@{}+$valid;$x.Remove('id');$x },
    { $x=[ordered]@{}+$valid;$x.timeout_seconds='180';$x },
    { $x=[ordered]@{}+$valid;$x.id='../bad';$x },
    { $x=[ordered]@{}+$valid;$x.arguments='-Mode';$x },
    { $x=[ordered]@{}+$valid;$x.timeout_seconds=3601;$x })
  foreach($make in $cases){ { Assert-ScenarioContract (& $make) } | Should -Throw }
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`

Expected: FAIL because schemas and strict validators do not exist.

- [ ] **Step 3: Create the complete scenario and input-manifest schemas.**

`scenario.schema.json`:

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["schema_version","id","platform","network_mode","timeout_seconds","declared_process","arguments","required_evidence"],"properties":{"schema_version":{"const":1},"id":{"type":"string","pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"},"platform":{"const":"windows"},"network_mode":{"enum":["offline","online"]},"timeout_seconds":{"type":"integer","minimum":1,"maximum":3600},"declared_process":{"type":"string","pattern":"^(?!.*(?:^|/)\\.\\.(?:/|$))(?!/)(?!.*//)[a-z0-9][a-z0-9._/-]*$"},"arguments":{"type":"array","maxItems":32,"items":{"type":"string","pattern":"^(?!.*(?:^|/)\\.\\.(?:/|$))(?!/)(?!.*//)[a-z0-9][a-z0-9._/-]*$"}},"required_evidence":{"type":"array","minItems":1,"maxItems":32,"uniqueItems":true,"items":{"type":"string","pattern":"^[a-z0-9][a-z0-9._-]*$"}}}}
```

`input-manifest.schema.json`:

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["schema_version","scenario_id","network_mode","source_revision","files","manifest_hash"],"properties":{"schema_version":{"const":1},"scenario_id":{"type":"string","pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"},"network_mode":{"enum":["offline","online"]},"source_revision":{"type":"string","pattern":"^[0-9a-f]{40}$"},"files":{"type":"array","minItems":1,"maxItems":4096,"items":{"type":"object","additionalProperties":false,"required":["path","length","sha256"],"properties":{"path":{"type":"string","pattern":"^(?!.*(?:^|/)\\.\\.(?:/|$))(?!/)(?!.*//)[a-z0-9][a-z0-9._/-]*$"},"length":{"type":"integer","minimum":0,"maximum":2147483648},"sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}}}},"manifest_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"}}}
```

`complete.schema.json`:

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["schema_version","run_id","scenario_id","profile","input_manifest_hash","configuration_hash","source_revision","declared_process_id","started_at_utc","ended_at_utc","cleanup_status","status","evidence_hashes","finalized_at_utc"],"properties":{"schema_version":{"const":1},"run_id":{"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"},"scenario_id":{"type":"string","pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"},"profile":{"enum":["offline","online"]},"input_manifest_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"configuration_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"source_revision":{"type":"string","pattern":"^[0-9a-f]{40}$"},"declared_process_id":{"type":"integer","minimum":1,"maximum":2147483647},"started_at_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"},"ended_at_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"},"cleanup_status":{"const":"succeeded"},"status":{"const":"complete"},"evidence_hashes":{"type":"object","minProperties":1,"maxProperties":35,"propertyNames":{"pattern":"^[a-z0-9][a-z0-9._-]*$"},"additionalProperties":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"finalized_at_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"}}}
```

- [ ] **Step 4: Implement explicit validators matching every schema keyword used.**

```powershell
function Assert-ExactProperties { param($Value,[string[]]$Required,[string[]]$Allowed) $names=@($Value.psobject.Properties.Name);foreach($n in $Required){if($names -cnotcontains $n){throw "Missing property: $n"}};foreach($n in $names){if($Allowed -cnotcontains $n){throw "Unknown property: $n"}} }
function Assert-String { param($Value,[string]$Name,[string]$Pattern,[int]$Min=0,[int]$Max=[int]::MaxValue) if($Value -isnot [string]-or $Value.Length -lt $Min-or $Value.Length -gt $Max-or $Value -cnotmatch $Pattern){throw "Invalid string: $Name"} }
function Assert-Integer { param($Value,[string]$Name,[int64]$Min,[int64]$Max) if($Value -isnot [int]-and $Value -isnot [long]){throw "Invalid integer type: $Name"};if([int64]$Value-lt $Min-or [int64]$Value-gt $Max){throw "Integer out of bounds: $Name"} }
function Assert-StringArray { param($Value,[string]$Name,[int]$Min,[int]$Max,[string]$Pattern,[switch]$Unique) if($Value -is [string]-or $Value -isnot [Array]){throw "Invalid array: $Name"};if($Value.Count-lt $Min-or $Value.Count-gt $Max){throw "Array bounds: $Name"};$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal);foreach($v in $Value){Assert-String $v $Name $Pattern 1 256;if($Unique-and -not $seen.Add($v)){throw "Duplicate array value: $Name"}} }
function Assert-ScenarioContract { param($Value) $r=@('schema_version','id','platform','network_mode','timeout_seconds','declared_process','arguments','required_evidence');Assert-ExactProperties $Value $r $r;if($Value.schema_version-ne 1-or $Value.platform-cne 'windows'){throw 'Invalid scenario constants'};Assert-String $Value.id id '^[a-z0-9]+(?:-[a-z0-9]+)*$';if(@('offline','online')-cnotcontains $Value.network_mode){throw 'Invalid network mode'};Assert-Integer $Value.timeout_seconds timeout_seconds 1 3600;$inputPattern='^(?!.*(?:^|/)\.\.(?:/|$))(?!/)(?!.*//)[a-z0-9][a-z0-9._/-]*$';Assert-String $Value.declared_process declared_process $inputPattern;Assert-StringArray $Value.arguments arguments 0 32 $inputPattern;Assert-StringArray $Value.required_evidence required_evidence 1 32 '^[a-z0-9][a-z0-9._-]*$' -Unique }
function Assert-InputManifestContract {
  param($Value)
  $r=@('schema_version','scenario_id','network_mode','source_revision','files','manifest_hash');Assert-ExactProperties $Value $r $r
  if($Value.schema_version-ne 1){throw 'Invalid manifest version'};Assert-String $Value.scenario_id scenario_id '^[a-z0-9]+(?:-[a-z0-9]+)*$';if(@('offline','online')-cnotcontains $Value.network_mode){throw 'Invalid network mode'};Assert-String $Value.source_revision source_revision '^[0-9a-f]{40}$';Assert-String $Value.manifest_hash manifest_hash '^[0-9a-f]{64}$'
  if($Value.files -is [string]-or $Value.files -isnot [Array]-or $Value.files.Count-lt 1-or $Value.files.Count-gt 4096){throw 'Invalid files array'};$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
  foreach($f in $Value.files){$fr=@('path','length','sha256');Assert-ExactProperties $f $fr $fr;Assert-String $f.path path '^(?!.*(?:^|/)\.\.(?:/|$))(?!/)(?!.*//)[a-z0-9][a-z0-9._/-]*$';if(-not $seen.Add($f.path)){throw 'Duplicate manifest path'};Assert-Integer $f.length length 0 2147483648;Assert-String $f.sha256 sha256 '^[0-9a-f]{64}$'}
  if((Get-Sha256Bytes (Get-ManifestPayloadBytes $Value))-cne $Value.manifest_hash){throw 'Manifest hash mismatch'}
}
function Assert-UtcTimestamp { param([string]$Value,[string]$Name) Assert-String $Value $Name '^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$';$dt=[datetime]::MinValue;if(-not [DateTime]::TryParseExact($Value,'o',[Globalization.CultureInfo]::InvariantCulture,[Globalization.DateTimeStyles]::AssumeUniversal,[ref]$dt)){throw "Invalid UTC timestamp: $Name"} }
function Assert-CompleteContract {
  param($Value)
  $r=@('schema_version','run_id','scenario_id','profile','input_manifest_hash','configuration_hash','source_revision','declared_process_id','started_at_utc','ended_at_utc','cleanup_status','status','evidence_hashes','finalized_at_utc');Assert-ExactProperties $Value $r $r
  if($Value.schema_version-ne 1-or $Value.cleanup_status-cne 'succeeded'-or $Value.status-cne 'complete'){throw 'Invalid completion constants'};Assert-RunId $Value.run_id|Out-Null;Assert-String $Value.scenario_id scenario_id '^[a-z0-9]+(?:-[a-z0-9]+)*$';if(@('offline','online')-cnotcontains $Value.profile){throw 'Invalid profile'}
  foreach($n in 'input_manifest_hash','configuration_hash'){Assert-String $Value.$n $n '^[0-9a-f]{64}$'};Assert-String $Value.source_revision source_revision '^[0-9a-f]{40}$';Assert-Integer $Value.declared_process_id declared_process_id 1 2147483647;foreach($n in 'started_at_utc','ended_at_utc','finalized_at_utc'){Assert-UtcTimestamp $Value.$n $n}
  $props=@($Value.evidence_hashes.psobject.Properties);if($props.Count-lt 1-or $props.Count-gt 35){throw 'Invalid evidence hash bounds'};foreach($p in $props){Assert-String $p.Name evidence_name '^[a-z0-9][a-z0-9._-]*$';Assert-String $p.Value evidence_hash '^[0-9a-f]{64}$'}
}
```

Tests mutate every property category in all three valid fixtures, including unknown nested file fields and an empty `evidence_hashes` object. Do not claim draft-2020-12 engine validation; schemas document the contracts and repository-owned validators enforce them without installation.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/schemas scripts/windows-sandbox/lib/Harness.Common.psm1 scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: define strict sandbox contracts"
```

Expected: PASS for unknown fields, required fields, exact types, patterns, arrays, uniqueness, and every bound.

### Task 3: Add Exact Profiles And The 10/18 Scenario Boundary

**Files:**
- Create: `scripts/windows-sandbox/templates/agent-manager-sandbox-offline.wsb.xml`
- Create: `scripts/windows-sandbox/templates/agent-manager-sandbox-online.wsb.xml`
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
- Modify: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** strict scenario validator and approved matrix.
**Produces:** exact profile routes, exact descriptor process routes, and 18 blocked/static macOS records.

- [ ] **Step 1: Write failing exact-route tests.**

```powershell
$routes=[ordered]@{'clean-install'='offline';'uac-accept'='offline';'uac-decline'='offline';'path-refresh'='offline';'multiple-node-installations'='offline';'proxy-failure'='online';'file-lock'='offline';'disk-space-guard'='offline';'batch-partial-failure'='offline';'postflight-path-version'='offline'}
$windows=Get-ChildItem "$PSScriptRoot/../scenarios/windows" -Filter *.json|ForEach-Object{Read-JsonFile $_.FullName}
@($windows).Count|Should -Be 10
foreach($d in $windows){Assert-ScenarioContract $d;$d.network_mode|Should -Be $routes[$d.id];$d.declared_process|Should -Be "tests/fixtures/input/fixture-$($d.id).ps1"}
$mac=(Read-JsonFile "$PSScriptRoot/../scenarios/macos/matrix.json").scenarios
@($mac|Where-Object architecture -ceq 'intel').Count|Should -Be 9;@($mac|Where-Object architecture -ceq 'apple_silicon').Count|Should -Be 9
@($mac|Where-Object{$_.execution_status-cne 'blocked'-or $_.evidence_kind-cne 'static_only'-or -not $_.requires_disposable_macos}).Count|Should -Be 0
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`

Expected: FAIL because profiles and scenario files do not exist.

- [ ] **Step 3: Create exact descriptors and hardened templates.**

Every Windows descriptor uses this complete shape, changing only `id`, route, fixture name, arguments, timeout, and required evidence:

```json
{"schema_version":1,"id":"proxy-failure","platform":"windows","network_mode":"online","timeout_seconds":180,"declared_process":"tests/fixtures/input/fixture-proxy-failure.ps1","arguments":["artifact.txt"],"required_evidence":["results.tsv","sandbox-transcript.txt"]}
```

Offline template:

```xml
<Configuration><VGpu>Disable</VGpu><Networking>Disable</Networking><ClipboardRedirection>Disable</ClipboardRedirection><PrinterRedirection>Disable</PrinterRedirection><AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput><MappedFolders><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders><LogonCommand><Command></Command></LogonCommand></Configuration>
```

The online template differs only in `<Networking>Enable</Networking>`. The macOS matrix has exactly 18 objects with `scenario_id`, `architecture`, `execution_status: "blocked"`, `evidence_kind: "static_only"`, and `requires_disposable_macos: true`.

- [ ] **Step 4: Run tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`

Expected: PASS for exact ten routes, exact process paths, template policy, nine Intel and nine Apple Silicon blocked/static records; no interactive acceptance is asserted.

- [ ] **Step 5: Commit profiles and matrix.**

```powershell
git add scripts/windows-sandbox/templates scripts/windows-sandbox/scenarios scripts/windows-sandbox/tests/Harness.Common.Tests.ps1 scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "feat: declare sandbox scenario boundary"
```

### Task 4: Stage Inputs Without Overwrite And Claim Runs Exclusively

**Files:**
- Create: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/input/runner.txt`
- Create: `scripts/windows-sandbox/tests/fixtures/input/artifact.txt`
- Create: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** tracked allowlist, descriptor, source revision, same-volume external runtime root, and common primitives.
**Produces:** immutable staged `test\input`, preserved prior generations, plus run `owner.claim`, `control`, and empty `sandbox-output`.

- [ ] **Step 1: Write failing concurrency, staging, and failure-preservation tests.**

```powershell
It 'allows exactly one claimant for a run ID' {
  $root=Join-Path $TestDrive 'evidence';[IO.Directory]::CreateDirectory($root)|Out-Null;$id='00000000-0000-0000-0000-000000000001'
  $jobs=1..2|ForEach-Object{Start-Job -ScriptBlock $claimScript -ArgumentList $root,$id}
  $results=@($jobs|Wait-Job|Receive-Job);@($results|Where-Object status -eq 'owned').Count|Should -Be 1;@($results|Where-Object status -eq 'collision').Count|Should -Be 1
  @(Get-ChildItem (Join-Path $root $id) -Force).Name|Should -Be @('control','owner.claim','sandbox-output')
}
It 'preserves prior input and partial staging on injected copy failure' {
  { Publish-StagedInput -TestRoot $root -CopyApi $failingCopy }|Should -Throw
  (Read-JsonFile (Join-Path $root 'input-current.json')).directory|Should -Be $oldDirectory
  Test-Path (Join-Path $root $oldDirectory)|Should -BeTrue;Test-Path $capturedStaging|Should -BeTrue
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`

Expected: FAIL because staging and ownership functions do not exist.

- [ ] **Step 3: Implement same-volume staging and immutable input selection.**

Generate `test\input-staging-<guid>` beside `test\input`, ensuring source and destination volume roots from `[IO.Path]::GetPathRoot()` are ordinal-ignore-case equal. Preflight the tracked allowlist of runner, selected scenario, common module, schemas, and declared fixture inputs; each source and every existing ancestor must pass `Assert-SafeTree`, each source must be a regular file, and each normalized destination must be unique and contained. Copy each allowlisted file with `[IO.File]::Open(source,Open,Read,Read)` to `[IO.File]::Open(destination,CreateNew,Write,None)`, creating each allowlisted directory once and rejecting reparses after creation.

Build and validate the manifest in staging, write `input-manifest.json` create-new, re-enumerate the complete staging tree, and reject anything except manifest-listed files plus the manifest itself. Never delete, replace, or merge an existing directory.

Open persistent `test\input-transition.lock` with `FileMode.OpenOrCreate`, read/write access, and `FileShare.None`; failure to obtain that exclusive handle blocks without changing input. While holding it, revalidate staging and any existing `test\input`. If existing input has the same valid manifest hash, preserve it, preserve staging for review, and map existing input. Otherwise preflight the absent destination `test\input-prior-<old_manifest_hash>` and both renames, rename existing input to that prior name, then rename staging to exact `test\input`. Renames are same-volume directory moves. Never delete or overwrite either generation; if the first rename succeeds and the second fails, preserve both paths, record a blocked transition, perform no rollback, and launch nothing. Close the lock handle in `finally`; the lock file remains. Exactly `test\input`, revalidated after the rename while the lock is held, is mapped read-only. Tests cover concurrent publishers, same-hash reuse, first/second rename failure, partial-state preservation, prior-name collision, and a successful second and third run.

- [ ] **Step 4: Implement exclusive run ownership before children.**

```powershell
function New-HarnessRun {
  param([string]$EvidenceRoot,[string]$RunId)
  Assert-RunId $RunId|Out-Null;Assert-SafeTree (Split-Path $EvidenceRoot -Parent) $EvidenceRoot|Out-Null
  $run=Join-Path $EvidenceRoot $RunId
  try{[IO.Directory]::CreateDirectory($run)|Out-Null;Write-BytesCreateNew (Join-Path $run 'owner.claim') ([Text.UTF8Encoding]::new($false).GetBytes($RunId))}catch{throw "Run ownership collision: $RunId"}
  try{[IO.Directory]::CreateDirectory((Join-Path $run 'control'))|Out-Null;[IO.Directory]::CreateDirectory((Join-Path $run 'sandbox-output'))|Out-Null}catch{throw "Owned run initialization failed: $RunId"}
  [pscustomobject]@{RunId=$RunId;RunRoot=$run;ClaimPath=(Join-Path $run 'owner.claim');ControlRoot=(Join-Path $run 'control');OutputRoot=(Join-Path $run 'sandbox-output')}
}
```

The shared `evidence` container is the only location touched before ownership. A loser may have participated in idempotent creation of the GUID directory, but failure to create `owner.claim` is final: it creates no child, control record, output, or file outside that shared run container. `owner.claim` deliberately contains only the run ID because it is adjacent to the mapped output; authoritative provenance remains create-new JSON under unmapped `control`. Tests synchronize two jobs at a barrier, repeat 100 times, and assert one owner, one loser, no loser writes, unchanged claim bytes, and no output contents.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/fixtures/input scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: stage and claim sandbox runs safely"
```

Expected: PASS for concurrent claims, reparse sources, duplicate destinations, copy failure, target collision, prior selector preservation, repeated runs, and exact mapped input selection.

### Task 5: Validate Configuration And Record Launch Chronology

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** owned run, exact descriptor/profile, selected input version, templates, and hashes.
**Produces:** validated WSB, create-new `control\launch-intent.json`, then create-new `control\process.json` after process creation.

- [ ] **Step 1: Write failing tamper and chronology tests.**

```powershell
It 'rejects every security-critical template mutation before process start' {
  foreach($mutation in $mutations){$template=New-TamperedTemplate $mutation;{New-Configuration -TemplatePath $template -Scenario $scenario -Profile offline -Run $run -InputRoot $input}|Should -Throw;$startCalls|Should -Be 0}
}
It 'writes intent before start and process identity after start' {
  Invoke-OwnedLaunch -Run $run -StartApi $fakeStart
  $events|Should -Be @('launch-intent.json','Start-Process','process.json')
  (Read-JsonFile (Join-Path $run.ControlRoot 'process.json')).sandbox_host_process_id|Should -Be 4242
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`

Expected: FAIL because configuration and launch functions do not exist.

- [ ] **Step 3: Generate, reload, and validate every configuration node.**

`New-Configuration` loads the exact route `templates\agent-manager-sandbox-$Profile.wsb.xml`, populates DOM text nodes, saves create-new under `control`, reloads that exact saved path, and rejects unless all conditions hold: root is exactly `Configuration`; `VGpu`, `ClipboardRedirection`, `PrinterRedirection`, `AudioInput`, and `VideoInput` are exactly `Disable`; `Networking` is exactly `Disable` offline or `Enable` online; `LogonCommand.Command` exactly equals the constructed runner command; there is exactly one `MappedFolders` node and exactly two `MappedFolder` children; map 0 has canonical exact host `InputRoot`, Sandbox path `C:\AgentManagerHarness\Input`, and `ReadOnly` text `true`; map 1 has canonical exact host `Run.OutputRoot`, Sandbox path `C:\AgentManagerHarness\Evidence`, and `ReadOnly` text `false`; no other element or attribute exists; neither run root nor control root is mapped; both host paths are non-overlapping and reparse-free.

Tests independently tamper each device, networking, command, root, attribute, element, map count/order, host path, Sandbox path, `ReadOnly`, extra map, parent map, and child map. Each case asserts `Start-Process` was never called.

- [ ] **Step 4: Implement immutable two-record launch provenance.**

Before launch, create `control\launch-intent.json` with exact fields `schema_version`, `run_id`, `scenario_id`, `profile`, `input_manifest_hash`, `configuration_hash`, `source_revision`, `output_relative_name`, `command`, and `intent_at_utc`; it has no PID. Call only `Start-Process -FilePath $wsbPath -PassThru`. Immediately after return, create `control\process.json` with `schema_version`, `run_id`, `sandbox_host_process_id`, `sandbox_host_process_start_time_utc` read from the returned process, and `recorded_at_utc`. If process-record creation fails, stop only the returned process after verifying its PID/start-time identity, write create-new `launch-failure.json` if possible, and block collection.

The collector consumes both immutable records. Runner completion copies the explicit run identity and configuration/input hashes passed in the logon command, but the host validates them against unmapped intent and process records. Tests prove an intent collision prevents launch, PID is absent from intent, process record cannot precede start, process collision triggers identity-checked stop, and no mutable record is used.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: validate and record sandbox launch"
```

### Task 6: Validate Declared Inputs And Clean Owned Process Trees

**Files:**
- Create: `scripts/windows-sandbox/Invoke-SandboxRunner.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/evidence/required.txt`
- Create: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`

**Consumes:** strict descriptor/manifest, read-only input root, explicit run identity/profile/hashes, and output mapping.
**Produces:** bounded fixture process execution, identity-aware descendant cleanup, failure evidence, and no broad kill.

- [ ] **Step 1: Write failing command-containment and process-tree tests.**

```powershell
It 'rejects traversal, absolute, unlisted, and non-fixture commands without launch' {
  foreach($p in '..\evil.exe','C:\Windows\System32\cmd.exe','missing.ps1','Invoke-SandboxRunner.ps1'){ {Resolve-DeclaredProcess -InputRoot $input -Manifest $manifest -DeclaredProcess $p}|Should -Throw };$startCalls|Should -Be 0
}
It 'stops descendants before parents by traversal depth and detects PID reuse' {
  $result=Stop-OwnedProcessTree -RootIdentity $root -Api $api -DeadlineUtc $deadline
  $calls|Should -Be @(13,12,11);$result.failed_process_ids|Should -Contain 14;$calls|Should -Not -Contain 99
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed`

Expected: FAIL because runner functions do not exist.

- [ ] **Step 3: Implement descriptor/process validation before launch.**

`Assert-RunnerPreflight` validates both contracts, recomputes manifest payload bytes and every listed file hash, rejects extras, validates profile equality, and resolves `declared_process` by replacing `/` with `\`, combining under `C:\AgentManagerHarness\Input`, and applying canonical containment/reparse checks. Its normalized relative path must exactly match one manifest entry, must begin `tests/fixtures/input/fixture-`, must end `.ps1`, and must equal `tests/fixtures/input/fixture-<scenario_id>.ps1`. Resolve every `arguments` entry through the same containment/reparse procedure and require an exact manifest entry; pass the resulting canonical input paths as positional arguments to the fixed trusted `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <declared fixture>` invocation. No argument is free-form or reparsed as command text. Tests cannot point process or arguments to the WSB, runner, product, installer, PowerShell executable, missing files, or arbitrary OS commands; fake `StartApi` is the only process API in Pester.

- [ ] **Step 4: Implement bounded, identity-aware post-order cleanup.**

Represent identity as `{process_id,parent_process_id,creation_time_utc}` from CIM. Starting at the returned root PID/start time, repeatedly snapshot descendants whose current parent identity matches the recorded parent and whose creation time is not earlier than the parent. Build `(identity, depth)` nodes by traversal, sort by depth descending using `[Array]::Sort` with a comparison delegate, and stop descendants before parents. Immediately before each stop, reload PID, parent PID, and creation time; mismatch records `pid_reused` and is never stopped.

After each deepest-first pass, rescan surviving owned parents for newly appearing children until two consecutive empty scans or the cleanup deadline. Wait only within the bounded deadline. Record each access denial, surviving identity, PID reuse, and late child in create-new `cleanup.json`; any such condition makes cleanup `failed`, prevents `complete.json`, and never falls back to image/product-name termination. Pester fake snapshots model 11 -> 12 -> 13, a late child 14, reused PID 12, access denial, disappearing nodes, and unrelated PID 99, and assert true traversal order rather than numeric PID order.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-SandboxRunner.ps1 scripts/windows-sandbox/tests/fixtures/evidence scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1
git commit -m "feat: enforce owned sandbox execution"
```

### Task 7: Finalize And Collect Complete Evidence

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-SandboxRunner.ps1`
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** required evidence, cleanup result, explicit run ID, unmapped claim/intent/process/manifest/configuration records, and one output child.
**Produces:** create-new `complete.json` written last by runner and create-new `control\collection.json` written after full host recomputation.

- [ ] **Step 1: Write failing finalization and collector tests.**

```powershell
It 'does not complete after cleanup failure or with an extra file' { {Finalize-Evidence -EvidenceRoot $root -CleanupStatus failed -RequiredEvidence @('results.tsv') -Identity $identity}|Should -Throw;Test-Path (Join-Path $root 'complete.json')|Should -BeFalse }
It 'rejects changed current evidence and impossible chronology' { {Assert-CollectedRun -RunId $id -EvidenceRoot $evidence}|Should -Throw '*hash*';Test-Path (Join-Path $control 'collection.json')|Should -BeFalse }
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1, .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`

Expected: FAIL because complete collection is not implemented.

- [ ] **Step 3: Implement runner complete-last semantics.**

Allowed output before completion is exactly descriptor `required_evidence` plus `started.json`, `runner-process.json`, and `cleanup.json`; `failure.json` is allowed only on a failed run and precludes completion. Re-enumerate with reparse checks, reject directories and any extra/unlisted file, require nonempty required evidence, and recalculate all current hashes. Validate `started <= process start <= ended <= finalized`, cleanup exactly `succeeded`, and explicit run identity. Construct the strict complete object, validate it, then write `complete.json` create-new as the final filesystem operation. No cleanup or evidence mutation follows it; cleanup failure, collision, or validation failure preserves partial output and precludes completion.

- [ ] **Step 4: Implement collector full recomputation from explicit identity.**

`Get-RunControlRecord` accepts only caller `RunId`, validates it, derives exactly `EvidenceRoot\RunId`, validates `owner.claim` bytes equal RunId, and derives only `control` and `sandbox-output`; it never enumerates `EvidenceRoot`. The collector validates strict intent, host-process, manifest, and complete contracts; recomputes configuration file hash and manifest payload/file hashes; compares every shared identity/hash/profile/revision field; separately validates host WSB PID/start identity and declared fixture PID; validates `intent <= sandbox_host_process_start <= started <= ended <= finalized <= collection` with at most 120 seconds host/Sandbox clock skew; rejects cleanup other than succeeded; re-enumerates output; rejects any directory, reparse, missing file, changed hash, or file not in the allowed complete set; and recalculates every current evidence hash rather than trusting prior values.

Only after all checks pass, write `control\collection.json` create-new with accepted hashes and collection UTC. A failed collection writes create-new `collection-failure.json` when possible, never `collection.json`, never changes output, and preserves all records. `control\archive.json` is an optional create-new index after accepted collection and contains the same explicit run ID/current hashes; it copies and extracts nothing.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1, .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-SandboxRunner.ps1 scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: finalize and collect sandbox evidence"
```

Expected: PASS for extra/missing/changed files, malformed contracts, run mismatch, timestamp inversion/skew, claim mismatch, configuration/manifest recomputation, cleanup failure, complete collision, and explicit-run-only collection.

### Task 8: Quarantine Legacy Files And Document Future Execution

**Files:**
- Create: `scripts/windows-sandbox/README.md`
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Modify: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** two exact legacy sources, explicit quarantine destinations, accepted static preflight, and new run control.
**Produces:** preflighted create-new provenance and fail-closed quarantine with blocked partial-move recovery, plus future-only commands.

- [ ] **Step 1: Write failing two-source preflight and partial-move tests.**

```powershell
It 'moves neither source when any destination exists' { {Move-LegacyHarnessToQuarantine -TestRoot $root -ControlRoot $control -RunId $id}|Should -Throw;Test-Path $wsb|Should -BeTrue;Test-Path $runner|Should -BeTrue }
It 'records a partial move as blocked without rollback or launch' { {Move-LegacyHarnessToQuarantine -TestRoot $root -ControlRoot $control -RunId $id -MoveApi $failSecond}|Should -Throw;(Read-JsonFile (Join-Path $control 'legacy-quarantine-failure.json')).status|Should -Be 'blocked';$launchCalls|Should -Be 0 }
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`

Expected: FAIL because quarantine and documentation do not exist.

- [ ] **Step 3: Implement fail-closed quarantine and provenance.**

Preflight both exact sources `agent-manager-test.wsb` and `run-agent-manager-sandbox.ps1` and both exact destinations under `test\retired\<RunId>` before either move: validate test/control/retired containment and every ancestor, reject reparses, require both regular sources, require destination directory newly created and empty, and require both targets absent. Hash both sources and write `control\legacy-inventory.json` create-new before moves. Move source one then source two without overwrite. After each move, verify destination hash and source absence. Write `legacy-quarantine.json` create-new only after both succeed.

If either move or verification fails, write `legacy-quarantine-failure.json` create-new with per-source states and `status: blocked`; preserve the partial source/destination state, perform no rollback, delete nothing, launch nothing, and require operator review. A preflight failure moves neither source. Tests inject first/second failures, destination collision, source disappearance, reparse, hash change, and provenance collision.

- [ ] **Step 4: Document future-only Windows commands and honest macOS boundary.**

README labels all ten launcher commands **FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION**, uses the exact authorized worktree, routes only `proxy-failure` online and the other nine offline, and states Sandbox/product/installers must not be launched by implementation or tests. It requires explicit returned run IDs for collection and forbids searching peer evidence directories. It documents rollback as selecting a validated tracked revision in an isolated checkout and creating a new input generation/run, never restoring the broad writable mapping. It states all 18 macOS cases require respective real disposable Intel/Apple Silicon hardware and static fixture/hash/ZIP/Mach-O/source observations do not satisfy interaction acceptance.

The README includes these exact gated commands:

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

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/README.md scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1 scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "docs: define safe sandbox migration"
```

### Task 9: Static Verification And Future Disposable Acceptance

**Files:**
- Create: `docs/superpowers/verification/2026-07-14-windows-sandbox-harness-hardening.md`
- Test: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** implementation commits, static tests, explicit future run IDs, and disposable platform environments.
**Produces:** static verification first; later, separately labeled Windows interaction evidence and blocked/observed native macOS evidence.

- [ ] **Step 1: Run static preflight without launching external processes.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests -Output Detailed
git diff --check
```

Expected: PASS using only fixtures, `TestDrive`, and mocked process APIs; record exact commit and results.

- [ ] **Step 2: Gate the ten Windows scenarios to a disposable Sandbox.**

Only after explicit operator approval, run the ten commands listed in README on a disposable Windows Sandbox host. Expected: each returns one explicit run ID; failures remain failed/blocked and preserved. Static success alone is not Windows interaction acceptance.

- [ ] **Step 3: Collect each Windows run only by its returned ID.**

For each returned ID, call `Get-RunControlRecord -RunId $runId -EvidenceRoot D:\codex\ai-deploy-toolkit\test\evidence`, then collect/archive that exact record. Expected: only fully recomputed, cleanup-successful evidence is accepted.

- [ ] **Step 4: Record native macOS outcomes without equivalence claims.**

Run nine Intel and nine Apple Silicon scenarios only on their respective real disposable hardware. Expected: unavailable cases stay `BLOCKED`; only native interaction logs/screenshots with OS and architecture can be accepted.

- [ ] **Step 5: Commit the verification record.**

```powershell
git add docs/superpowers/verification/2026-07-14-windows-sandbox-harness-hardening.md
git commit -m "docs: verify sandbox harness hardening"
```

## Acceptance Criteria And Self-Review

- Alternative B and the locked file tree remain unchanged: immutable read-only input, unmapped control, and one fresh writable output.
- Run ownership is a create-new marker before child creation; concurrency has one owner, losers fail closed, and provenance is reconciled with unmapped control.
- Launch chronology uses immutable intent before launch and immutable process identity after launch.
- Manifest ordering is ordinal and its exact BOM-free payload bytes exclude `manifest_hash`; host and Sandbox reconstruct identical bytes.
- Runtime enforcement uses repository-owned explicit validators matching complete documentation schemas, with adversarial coverage of fields, types, patterns, arrays, uniqueness, and bounds.
- Configuration reload validates every exact security node, command, mapping, `ReadOnly` value, and absence of extras before process start.
- Staging is same-volume, allowlisted, reparse-safe, immutable, failure-preserving, and never deletes or overwrites prior input.
- Cleanup is identity-aware post-order traversal with bounded rescans, PID-reuse defense, no broad kill, and completion blocked on any failure.
- Declared process and arguments are manifest-contained fixture inputs, so tests cannot launch Sandbox or product commands.
- Finalization and collection re-enumerate and rehash all current evidence, validate chronology/run identity, reject extras, and write completion last only after cleanup success.
- Legacy quarantine preflights both sources/destinations, uses create-new provenance, blocks partial moves without rollback, and never launches or deletes legacy files.
- Review every snippet for Windows PowerShell 5.1 syntax/API availability, defined helpers, TOCTOU boundaries, path-root edges, reparse traversal, safe move behavior, matching field names, and honest expected results.

Plan complete and saved to `docs/superpowers/plans/2026-07-14-windows-sandbox-harness-hardening.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task and review each task before the next.
2. Inline Execution - execute tasks in this session using executing-plans with review checkpoints.
