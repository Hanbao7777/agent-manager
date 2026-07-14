# Windows Sandbox Harness Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a versioned, fail-closed Windows Sandbox harness that stages hash-validated immutable inputs and accepts only complete evidence from one exclusively claimed, per-run writable output directory.

**Architecture:** Preserve approved Alternative B: tracked source under `scripts/windows-sandbox` is staged into one immutable external input generation selected by a managed record and mapped read-only, while each GUID run owns an unmapped `control` child and one fresh writable `sandbox-output` child. A create-new ownership record exclusively claims the run before children exist; immutable intent/process records resolve launch chronology, and both host and Sandbox use repository-owned strict validators plus identical canonical manifest bytes.

**Tech Stack:** Windows PowerShell 5.1, Pester 5, Windows Sandbox XML, SHA-256, Git; no runtime package installation and no unprovided JSON Schema engine.

**Global Constraints:**

- Run every repository command from `D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator`; never operate from another checkout.
- `D:\codex\ai-deploy-toolkit\test` is external runtime data. Only `scripts\windows-sandbox` is tracked source; never map or commit the external test-root parent.
- A run ID is a newly generated lowercase GUID in canonical `D` form. The only writable Sandbox mapping is that run's fresh `sandbox-output` child.
- Offline is default. Only descriptor `network_mode: "online"` permits the online template, and caller/profile disagreement fails before launch.
- Both profiles disable clipboard, printer, audio input, video input, and vGPU. They contain exactly two maps: the generation named by validated `test\input-current.json` read-only at `C:\AgentManagerHarness\Input`, and `sandbox-output` read-write at `C:\AgentManagerHarness\Evidence`.
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
  scenario-drivers/windows/
    clean-install.ps1                            # Future-only clean product installation and resulting state capture.
    uac-accept.ps1                               # Future-only accepted elevation path and result capture.
    uac-decline.ps1                              # Future-only declined elevation path and no-change proof.
    path-refresh.ps1                             # Future-only PATH refresh and inherited-process observation.
    multiple-node-installations.ps1              # Future-only multiple Node discovery/selection observation.
    proxy-failure.ps1                            # Future-only online-profile proxy failure behavior.
    file-lock.ps1                                # Future-only locked-file failure and recovery observation.
    disk-space-guard.ps1                         # Future-only insufficient-space guard behavior.
    batch-partial-failure.ps1                    # Future-only batch partial-failure accounting.
    postflight-path-version.ps1                  # Future-only postflight PATH and version verification.
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
**Produces:** canonical containment/reparse guards, a non-following tree walker, create-new byte/JSON writers, ordinal sorting, and one manifest-byte algorithm used on host and in Sandbox.

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
    $fixture=Join-Path $TestDrive 'ordinal';[IO.Directory]::CreateDirectory($fixture)|Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'z.txt'),'z');[IO.File]::WriteAllText((Join-Path $fixture 'i.txt'),'i')
    $old=[Globalization.CultureInfo]::CurrentCulture
    try { [Globalization.CultureInfo]::CurrentCulture=[Globalization.CultureInfo]::GetCultureInfo('tr-TR');$m=Get-DeterministicManifest -InputRoot $fixture -ScenarioId clean-install -NetworkMode offline -SourceRevision ('a'*40);$m.files.path|Should -Be @('i.txt','z.txt');$saved=Join-Path $TestDrive 'manifest.json';Write-JsonCreateNew $saved $m;(Get-ManifestPayloadBytes $m)|Should -Be (Get-ManifestPayloadBytes (Read-JsonFile $saved)) } finally { [Globalization.CultureInfo]::CurrentCulture=$old }
  }
  It 'rejects a reparse directory before descent' {
    $calls=[Collections.Generic.List[string]]::new();$root=Join-Path $TestDrive 'tree';[IO.Directory]::CreateDirectory($root)|Out-Null;$link=Join-Path $root 'link'
    $api=@{Entries={param($p)$calls.Add($p);if($p-ceq $root){@($link)}else{@()}};Item={param($p)if($p-ceq $root){[pscustomobject]@{FullName=$p;PSIsContainer=$true;Attributes=[IO.FileAttributes]::Directory}}else{[pscustomobject]@{FullName=$p;PSIsContainer=$true;Attributes=([IO.FileAttributes]::Directory-bor[IO.FileAttributes]::ReparsePoint)}}}}
    { Get-SafeRegularFiles -Root $root -FileSystemApi $api }|Should -Throw '*Reparse point*';$calls|Should -Be @($root)
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
function Get-SafeRegularFiles {
  param([string]$Root,$FileSystemApi)
  $rootPath=ConvertTo-CanonicalPath $Root;Assert-SafeTree $rootPath $rootPath -AllowRoot|Out-Null
  if($null-eq $FileSystemApi){$FileSystemApi=@{Entries={param($p)[IO.Directory]::EnumerateFileSystemEntries($p)};Item={param($p)Get-Item -LiteralPath $p -Force -ErrorAction Stop}}}
  $pending=[Collections.Generic.Stack[string]]::new();$pending.Push($rootPath);$files=[Collections.Generic.List[IO.FileInfo]]::new()
  while($pending.Count){$dir=$pending.Pop();$dirItem=& $FileSystemApi['Item'] $dir;if(($dirItem.Attributes-band [IO.FileAttributes]::ReparsePoint)-ne 0){throw "Reparse point rejected: $dir"};if(-not $dirItem.PSIsContainer){throw "Expected directory: $dir"}
    foreach($entryPath in & $FileSystemApi['Entries'] $dir){$full=Assert-ContainedPath $rootPath $entryPath;$item=& $FileSystemApi['Item'] $full;if(($item.Attributes-band [IO.FileAttributes]::ReparsePoint)-ne 0){throw "Reparse point rejected: $full"};if($item.PSIsContainer){$pending.Push($full)}elseif($item -is [IO.FileInfo]){$files.Add($item)}else{throw "Non-regular file rejected: $full"}}
  };$files
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
  foreach($file in Get-SafeRegularFiles -Root $root){Assert-SafeTree $root $file.FullName|Out-Null;$name=$file.FullName.Substring($root.Length).TrimStart('\').Replace('\','/').ToLowerInvariant();if($byName.ContainsKey($name)){throw "Duplicate normalized input: $name"};$byName[$name]=[ordered]@{path=$name;length=[int64]$file.Length;sha256=Get-Sha256Hex $file.FullName};$names.Add($name)}
  $m=[ordered]@{schema_version=1;scenario_id=$ScenarioId;network_mode=$NetworkMode;source_revision=$SourceRevision;files=@(Sort-OrdinalStrings $names|ForEach-Object{$byName[$_]})}; $m.manifest_hash=Get-Sha256Bytes (Get-ManifestPayloadBytes $m); $m
}
```

The hashed bytes are exactly UTF-8 without BOM of compressed ordered JSON containing `schema_version`, `scenario_id`, `network_mode`, `source_revision`, and `files`, in that order. `manifest_hash` is excluded from those bytes, then appended as the final top-level property; validation reconstructs the payload object and bytes, never hashes the serialized on-disk object wholesale.

- [ ] **Step 4: Run tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`

Expected: PASS for prefix siblings, root equality, reparse ancestors, a junction or symlink created when privileges permit, a mocked reparse directory otherwise, canonical GUIDs, create-new collisions, Turkish-culture ordering, duplicate normalized names, and host/Sandbox payload-byte equality. The walker test proves the reparse target is never enumerated; staging validation and collection reuse `Get-SafeRegularFiles` rather than recursive provider enumeration.

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
**Produces:** documentation schemas plus explicit strict validators for descriptors, manifests, completion, run ownership, launch intent, host process, collection, and failure records; the validators, not PowerShell itself, are runtime enforcement.

- [ ] **Step 1: Write table-driven failing contract tests.**

```powershell
$valid=[ordered]@{schema_version=1;id='clean-install';platform='windows';network_mode='offline';timeout_seconds=180;declared_process='scenario-drivers/windows/clean-install.ps1';arguments=@('artifacts/agent-manager.msi');required_evidence=@('results.tsv','sandbox-transcript.txt')}
function Copy-TestValue { param($Value) ($Value|ConvertTo-Json -Depth 32|ConvertFrom-Json) }
It 'rejects unknown, absent, wrong-type, pattern, array, and bound violations' {
  $cases=@(
    { $x=Copy-TestValue $valid;$x|Add-Member extra 1;$x },
    { $x=Copy-TestValue $valid;$x.psobject.Properties.Remove('id');$x },
    { $x=Copy-TestValue $valid;$x.timeout_seconds='180';$x },
    { $x=Copy-TestValue $valid;$x.id='../bad';$x },
    { $x=Copy-TestValue $valid;$x.arguments='-Mode';$x },
    { $x=Copy-TestValue $valid;$x.timeout_seconds=3601;$x })
  foreach($make in $cases){ { Assert-ScenarioContract (& $make) } | Should -Throw }
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed`

Expected: FAIL because schemas and strict validators do not exist.

- [ ] **Step 3: Create the complete scenario and input-manifest schemas.**

`scenario.schema.json`:

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["schema_version","id","platform","network_mode","timeout_seconds","declared_process","arguments","required_evidence"],"properties":{"schema_version":{"const":1},"id":{"type":"string","pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"},"platform":{"const":"windows"},"network_mode":{"enum":["offline","online"]},"timeout_seconds":{"type":"integer","minimum":1,"maximum":3600},"declared_process":{"type":"string","pattern":"^scenario-drivers/windows/[a-z0-9]+(?:-[a-z0-9]+)*\\.ps1$"},"arguments":{"type":"array","maxItems":32,"items":{"type":"string","pattern":"^artifacts/[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)*$"}},"required_evidence":{"type":"array","minItems":1,"maxItems":32,"uniqueItems":true,"items":{"type":"string","pattern":"^[a-z0-9][a-z0-9._-]*$"}}}}
```

`input-manifest.schema.json`:

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["schema_version","scenario_id","network_mode","source_revision","files","manifest_hash"],"properties":{"schema_version":{"const":1},"scenario_id":{"type":"string","pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"},"network_mode":{"enum":["offline","online"]},"source_revision":{"type":"string","pattern":"^[0-9a-f]{40}$"},"files":{"type":"array","minItems":1,"maxItems":4096,"items":{"type":"object","additionalProperties":false,"required":["path","length","sha256"],"properties":{"path":{"type":"string","pattern":"^(?!.*(?:^|/)\\.\\.(?:/|$))(?!/)(?!.*//)[a-z0-9][a-z0-9._/-]*$"},"length":{"type":"integer","minimum":0,"maximum":2147483648},"sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}}}},"manifest_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"}}}
```

`complete.schema.json`:

```json
{"$schema":"https://json-schema.org/draft/2020-12/schema","type":"object","additionalProperties":false,"required":["schema_version","run_id","scenario_id","profile","input_manifest_hash","configuration_hash","source_revision","driver_process_id","driver_process_start_time_utc","started_at_utc","ended_at_utc","cleanup_status","status","evidence_hashes","finalized_at_utc"],"properties":{"schema_version":{"const":1},"run_id":{"type":"string","pattern":"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"},"scenario_id":{"type":"string","pattern":"^[a-z0-9]+(?:-[a-z0-9]+)*$"},"profile":{"enum":["offline","online"]},"input_manifest_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"configuration_hash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"source_revision":{"type":"string","pattern":"^[0-9a-f]{40}$"},"driver_process_id":{"type":"integer","minimum":1,"maximum":2147483647},"driver_process_start_time_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"},"started_at_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"},"ended_at_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"},"cleanup_status":{"const":"succeeded"},"status":{"const":"complete"},"evidence_hashes":{"type":"object","minProperties":1,"maxProperties":35,"propertyNames":{"pattern":"^[a-z0-9][a-z0-9._-]*$"},"additionalProperties":{"type":"string","pattern":"^[0-9a-f]{64}$"}},"finalized_at_utc":{"type":"string","format":"date-time","pattern":"^[0-9]{4}-[0-9]{2}-[0-9]{2}T.*Z$"}}}
```

- [ ] **Step 4: Implement explicit validators matching every schema keyword used.**

```powershell
function Assert-ExactProperties { param($Value,[string[]]$Required,[string[]]$Allowed) $names=@($Value.psobject.Properties.Name);foreach($n in $Required){if($names -cnotcontains $n){throw "Missing property: $n"}};foreach($n in $names){if($Allowed -cnotcontains $n){throw "Unknown property: $n"}} }
function Assert-String { param($Value,[string]$Name,[string]$Pattern,[int]$Min=0,[int]$Max=[int]::MaxValue) if($Value -isnot [string]-or $Value.Length -lt $Min-or $Value.Length -gt $Max-or $Value -cnotmatch $Pattern){throw "Invalid string: $Name"} }
function Assert-Integer { param($Value,[string]$Name,[int64]$Min,[int64]$Max) if($Value -isnot [int]-and $Value -isnot [long]){throw "Invalid integer type: $Name"};if([int64]$Value-lt $Min-or [int64]$Value-gt $Max){throw "Integer out of bounds: $Name"} }
function Assert-StringArray { param($Value,[string]$Name,[int]$Min,[int]$Max,[string]$Pattern,[switch]$Unique) if($Value -is [string]-or $Value -isnot [Array]){throw "Invalid array: $Name"};if($Value.Count-lt $Min-or $Value.Count-gt $Max){throw "Array bounds: $Name"};$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal);foreach($v in $Value){Assert-String $v $Name $Pattern 1 256;if($Unique-and -not $seen.Add($v)){throw "Duplicate array value: $Name"}} }
function Assert-ScenarioContract { param($Value) $r=@('schema_version','id','platform','network_mode','timeout_seconds','declared_process','arguments','required_evidence');Assert-ExactProperties $Value $r $r;if($Value.schema_version-ne 1-or $Value.platform-cne 'windows'){throw 'Invalid scenario constants'};Assert-String $Value.id id '^[a-z0-9]+(?:-[a-z0-9]+)*$';if(@('offline','online')-cnotcontains $Value.network_mode){throw 'Invalid network mode'};Assert-Integer $Value.timeout_seconds timeout_seconds 1 3600;Assert-String $Value.declared_process declared_process '^scenario-drivers/windows/[a-z0-9]+(?:-[a-z0-9]+)*\.ps1$';Assert-StringArray $Value.arguments arguments 0 32 '^artifacts/[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)*$';Assert-StringArray $Value.required_evidence required_evidence 1 32 '^[a-z0-9][a-z0-9._-]*$' -Unique }
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
  $r=@('schema_version','run_id','scenario_id','profile','input_manifest_hash','configuration_hash','source_revision','driver_process_id','driver_process_start_time_utc','started_at_utc','ended_at_utc','cleanup_status','status','evidence_hashes','finalized_at_utc');Assert-ExactProperties $Value $r $r
  if($Value.schema_version-ne 1-or $Value.cleanup_status-cne 'succeeded'-or $Value.status-cne 'complete'){throw 'Invalid completion constants'};Assert-RunId $Value.run_id|Out-Null;Assert-String $Value.scenario_id scenario_id '^[a-z0-9]+(?:-[a-z0-9]+)*$';if(@('offline','online')-cnotcontains $Value.profile){throw 'Invalid profile'}
  foreach($n in 'input_manifest_hash','configuration_hash'){Assert-String $Value.$n $n '^[0-9a-f]{64}$'};Assert-String $Value.source_revision source_revision '^[0-9a-f]{40}$';Assert-Integer $Value.driver_process_id driver_process_id 1 2147483647;foreach($n in 'driver_process_start_time_utc','started_at_utc','ended_at_utc','finalized_at_utc'){Assert-UtcTimestamp $Value.$n $n}
  $props=@($Value.evidence_hashes.psobject.Properties);if($props.Count-lt 1-or $props.Count-gt 35){throw 'Invalid evidence hash bounds'};foreach($p in $props){Assert-String $p.Name evidence_name '^[a-z0-9][a-z0-9._-]*$';Assert-String $p.Value evidence_hash '^[0-9a-f]{64}$'}
}
function Assert-AuthorityRecord {
  param([ValidateSet('owner','launch-intent','process','runner-process','cleanup','collection','failure')][string]$Kind,$Value)
  $sets=@{
    owner=@('schema_version','run_id','owner_nonce','claimed_at_utc')
    'launch-intent'=@('schema_version','run_id','scenario_id','profile','input_manifest_hash','configuration_hash','source_revision','output_relative_name','windows_sandbox_executable','wsb_path','intent_at_utc')
    process=@('schema_version','run_id','sandbox_host_process_id','sandbox_host_process_start_time_utc','recorded_at_utc')
    'runner-process'=@('schema_version','run_id','scenario_id','driver_path','driver_process_id','driver_process_start_time_utc','started_at_utc')
    cleanup=@('schema_version','run_id','driver_process_id','driver_process_start_time_utc','exit_code','cleanup_status','failed_processes','ended_at_utc')
    collection=@('schema_version','run_id','status','input_manifest_hash','configuration_hash','source_revision','evidence_hashes','collected_at_utc')
    failure=@('schema_version','run_id','stage','status','reason','details_hash','recorded_at_utc')
  };$names=$sets[$Kind];Assert-ExactProperties $Value $names $names;if($Value.schema_version-ne 1){throw 'Invalid authority version'};Assert-RunId $Value.run_id|Out-Null
  switch($Kind){
    owner {Assert-String $Value.owner_nonce owner_nonce '^[0-9a-f]{32}$';Assert-UtcTimestamp $Value.claimed_at_utc claimed_at_utc}
    'launch-intent' {Assert-String $Value.scenario_id scenario_id '^[a-z0-9]+(?:-[a-z0-9]+)*$';if(@('offline','online')-cnotcontains $Value.profile){throw 'Invalid profile'};foreach($n in 'input_manifest_hash','configuration_hash'){Assert-String $Value.$n $n '^[0-9a-f]{64}$'};Assert-String $Value.source_revision source_revision '^[0-9a-f]{40}$';if($Value.output_relative_name-cne 'sandbox-output'){throw 'Invalid output name'};foreach($n in 'windows_sandbox_executable','wsb_path'){Assert-String $Value.$n $n '^[A-Za-z]:\\[^\r\n"]+$'};Assert-UtcTimestamp $Value.intent_at_utc intent_at_utc}
    process {Assert-Integer $Value.sandbox_host_process_id sandbox_host_process_id 1 2147483647;Assert-UtcTimestamp $Value.sandbox_host_process_start_time_utc sandbox_host_process_start_time_utc;Assert-UtcTimestamp $Value.recorded_at_utc recorded_at_utc}
    'runner-process' {Assert-String $Value.scenario_id scenario_id '^[a-z0-9]+(?:-[a-z0-9]+)*$';Assert-String $Value.driver_path driver_path '^scenario-drivers/windows/[a-z0-9-]+\.ps1$';Assert-Integer $Value.driver_process_id driver_process_id 1 2147483647;Assert-UtcTimestamp $Value.driver_process_start_time_utc driver_process_start_time_utc;Assert-UtcTimestamp $Value.started_at_utc started_at_utc}
    cleanup {Assert-Integer $Value.driver_process_id driver_process_id 1 2147483647;Assert-UtcTimestamp $Value.driver_process_start_time_utc driver_process_start_time_utc;Assert-Integer $Value.exit_code exit_code -2147483648 2147483647;if(@('succeeded','failed')-cnotcontains $Value.cleanup_status){throw 'Invalid cleanup status'};if($Value.failed_processes -isnot [Array]-or $Value.failed_processes.Count-gt 256){throw 'Invalid failed processes'};foreach($p in $Value.failed_processes){$pn=@('process_id','creation_time_utc','reason');Assert-ExactProperties $p $pn $pn;Assert-Integer $p.process_id process_id 1 2147483647;Assert-UtcTimestamp $p.creation_time_utc creation_time_utc;Assert-String $p.reason reason '^(access-denied|pid-reused|survived|late-child)$'};Assert-UtcTimestamp $Value.ended_at_utc ended_at_utc}
    collection {if($Value.status-cne 'accepted'){throw 'Invalid collection status'};foreach($n in 'input_manifest_hash','configuration_hash'){Assert-String $Value.$n $n '^[0-9a-f]{64}$'};Assert-String $Value.source_revision source_revision '^[0-9a-f]{40}$';$hashes=@($Value.evidence_hashes.psobject.Properties);if($hashes.Count-lt 1-or $hashes.Count-gt 35){throw 'Invalid collected hashes'};foreach($p in $hashes){Assert-String $p.Name evidence_name '^[a-z0-9][a-z0-9._-]*$';Assert-String $p.Value evidence_hash '^[0-9a-f]{64}$'};Assert-UtcTimestamp $Value.collected_at_utc collected_at_utc}
    failure {Assert-String $Value.stage stage '^[a-z0-9-]+$';if(@('blocked','failed')-cnotcontains $Value.status){throw 'Invalid failure status'};Assert-String $Value.reason reason '^.{1,512}$' 1 512;Assert-String $Value.details_hash details_hash '^[0-9a-f]{64}$';Assert-UtcTimestamp $Value.recorded_at_utc recorded_at_utc}
  }
}
```

Every authority record is written once with `Write-JsonCreateNew`, read back, validated with its exact kind, and never amended. Tests mutate every required field, add an unknown field, change each type/pattern/constant, collide the target, and exercise an empty or oversized hash map across the three schema contracts and seven authority kinds. `launch-failure.json`, `collection-failure.json`, and `legacy-quarantine-failure.json` all use the `failure` field set; stage-specific detail is a separate create-new JSON file bound by `details_hash`, not mutable acceptance authority. Do not claim draft-2020-12 engine validation; schemas document those three contracts and repository-owned validators enforce all contracts without installation.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/schemas scripts/windows-sandbox/lib/Harness.Common.psm1 scripts/windows-sandbox/tests/Harness.Common.Tests.ps1
git commit -m "feat: define strict sandbox contracts"
```

Expected: PASS for unknown fields, required fields, exact types, patterns, arrays, uniqueness, every bound, immutable collisions, and all seven authority-record field sets.

### Task 3: Add Production Drivers, Exact Profiles, And The 10/18 Boundary

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
- Create: `scripts/windows-sandbox/scenario-drivers/windows/clean-install.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/uac-accept.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/uac-decline.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/path-refresh.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/multiple-node-installations.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/proxy-failure.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/file-lock.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/disk-space-guard.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/batch-partial-failure.ps1`
- Create: `scripts/windows-sandbox/scenario-drivers/windows/postflight-path-version.ps1`
- Create: `scripts/windows-sandbox/scenarios/macos/matrix.json`
- Create: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`
- Modify: `scripts/windows-sandbox/tests/Harness.Common.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1`

**Consumes:** strict scenario validator and approved matrix.
**Produces:** ten production descriptor-to-driver routes, exact profiles, safe driver interfaces, and 18 blocked/static macOS records.

- [ ] **Step 1: Write failing exact-route tests.**

```powershell
$routes=[ordered]@{'clean-install'='offline';'uac-accept'='offline';'uac-decline'='offline';'path-refresh'='offline';'multiple-node-installations'='offline';'proxy-failure'='online';'file-lock'='offline';'disk-space-guard'='offline';'batch-partial-failure'='offline';'postflight-path-version'='offline'}
$windows=Get-ChildItem "$PSScriptRoot/../scenarios/windows" -Filter *.json|ForEach-Object{Read-JsonFile $_.FullName}
@($windows).Count|Should -Be 10
foreach($d in $windows){Assert-ScenarioContract $d;$d.network_mode|Should -Be $routes[$d.id];$d.declared_process|Should -Be "scenario-drivers/windows/$($d.id).ps1";$d.declared_process|Should -Not -Match '^tests/'}
$mac=(Read-JsonFile "$PSScriptRoot/../scenarios/macos/matrix.json").scenarios
@($mac|Where-Object architecture -ceq 'intel').Count|Should -Be 9;@($mac|Where-Object architecture -ceq 'apple_silicon').Count|Should -Be 9
@($mac|Where-Object{$_.execution_status-cne 'blocked'-or $_.evidence_kind-cne 'static_only'-or -not $_.requires_disposable_macos}).Count|Should -Be 0
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`

Expected: FAIL because profiles and scenario files do not exist.

- [ ] **Step 3: Create production drivers, exact descriptors, and hardened templates.**

Every Windows descriptor uses this complete shape, changing only `id`, route, production driver, manifest-listed artifact arguments, timeout, and required evidence:

```json
{"schema_version":1,"id":"proxy-failure","platform":"windows","network_mode":"online","timeout_seconds":180,"declared_process":"scenario-drivers/windows/proxy-failure.ps1","arguments":["artifacts/agent-manager.msi"],"required_evidence":["results.tsv","sandbox-transcript.txt"]}
```

Offline template:

```xml
<Configuration><VGpu>Disable</VGpu><Networking>Disable</Networking><ClipboardRedirection>Disable</ClipboardRedirection><PrinterRedirection>Disable</PrinterRedirection><AudioInput>Disable</AudioInput><VideoInput>Disable</VideoInput><MappedFolders><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder><MappedFolder><HostFolder></HostFolder><SandboxFolder>C:\AgentManagerHarness\Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder></MappedFolders><LogonCommand><Command></Command></LogonCommand></Configuration>
```

The online template differs only in `<Networking>Enable</Networking>`. Each production driver has `[CmdletBinding()] param([Parameter(Mandatory=$true)][string[]]$ArtifactPaths,[Parameter(Mandatory=$true)][string]$EvidenceRoot,[Parameter(Mandatory=$true)][hashtable]$Api)`, requires `EvidenceRoot` to equal the runner's fixed mapped evidence path, rejects artifact paths not pre-resolved by the runner, and calls only injected `$Api` operations. `clean-install` installs and records product/version/PATH; `uac-accept` records the accepted elevation path; `uac-decline` records refusal and unchanged state; `path-refresh` compares parent/new-process PATH; `multiple-node-installations` inventories and verifies selection; `proxy-failure` applies the declared proxy and records failure; `file-lock` owns a test lock and records recovery; `disk-space-guard` supplies bounded free-space observations; `batch-partial-failure` records each item outcome; `postflight-path-version` records final PATH and version. The runner constructs the fixed API adapter; neither descriptor nor artifact can supply functions. Real adapters may invoke product, MSI, WebView, and owned processes only after future Sandbox preflight; Pester dot-sources no production driver and tests driver routing/content plus mocked API contracts without executing actions. A driver-unit success is not scenario acceptance; only future interactive evidence accepted by the collector can satisfy a Windows scenario. The macOS matrix has exactly 18 objects with `scenario_id`, `architecture`, `execution_status: "blocked"`, `evidence_kind: "static_only"`, and `requires_disposable_macos: true`.

- [ ] **Step 4: Run tests and verify green.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Harness.Common.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`

Expected: PASS for ten production drivers, exact routes and profiles, no descriptor under `tests`, safe injected interfaces, template policy, and nine Intel plus nine Apple Silicon blocked/static records. Tests inspect files and use mocks only; they neither execute drivers nor assert interactive acceptance.

- [ ] **Step 5: Commit profiles and matrix.**

```powershell
git add scripts/windows-sandbox/templates scripts/windows-sandbox/scenarios scripts/windows-sandbox/scenario-drivers scripts/windows-sandbox/tests/Harness.Common.Tests.ps1 scripts/windows-sandbox/tests/Scenario.Matrix.Tests.ps1
git commit -m "feat: declare sandbox scenario boundary"
```

### Task 4: Stage Managed Input Generations And Claim Runs Exclusively

**Files:**
- Create: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/input/runner.txt`
- Create: `scripts/windows-sandbox/tests/fixtures/input/artifact.txt`
- Create: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** exact tracked runner/common/schemas/selected production driver, descriptor, strict external `test\artifact-manifest.json`, reparse-safe `test\artifact-sources`, source revision, same-volume external runtime root, and common primitives.
**Produces:** immutable `test\input-generations\input-<manifest_hash>-<guid>`, strict `input-current.json`, preserved prior selectors/generations, plus run `owner.json`, `control`, and empty `sandbox-output`.

- [ ] **Step 1: Write failing concurrency, staging, and failure-preservation tests.**

```powershell
It 'allows exactly one claimant for a run ID' {
  $root=Join-Path $TestDrive 'evidence';[IO.Directory]::CreateDirectory($root)|Out-Null;$id='00000000-0000-0000-0000-000000000001'
  $launcher=(Resolve-Path "$PSScriptRoot/../Invoke-AgentManagerSandbox.ps1").Path;$claimScript={param($script,$root,$id). $script;try{New-HarnessRun -EvidenceRoot $root -RunId $id|Out-Null;'owned'}catch{'collision'}}
  $jobs=1..2|ForEach-Object{Start-Job -ScriptBlock $claimScript -ArgumentList $launcher,$root,$id}
  $results=@($jobs|Wait-Job|Receive-Job);@($results|Where-Object status -eq 'owned').Count|Should -Be 1;@($results|Where-Object status -eq 'collision').Count|Should -Be 1
  @(Get-ChildItem (Join-Path $root $id) -Force).Name|Should -Be @('control','owner.json','sandbox-output')
}
It 'rejects malicious lock and selector paths without mutation' {
  $root=Join-Path $TestDrive 'test';$source=Join-Path $TestDrive 'source';[IO.Directory]::CreateDirectory($root)|Out-Null;[IO.Directory]::CreateDirectory($source)|Out-Null
  $sentinel=Join-Path $root 'sentinel.txt';[IO.File]::WriteAllText($sentinel,'unchanged');$before=Get-Sha256Hex $sentinel;$startCalls=0
  $pathApi=@{GetItem={param($p)if($p-like '*input-transition.lock'){[pscustomobject]@{PSIsContainer=$false;Attributes=[IO.FileAttributes]::ReparsePoint}}else{Get-Item -LiteralPath $p -Force}}}
  { Publish-StagedInput -TestRoot $root -SourceRoot $source -PathApi $pathApi }|Should -Throw '*Reparse point*'
  (Get-Sha256Hex $sentinel)|Should -Be $before;$startCalls|Should -Be 0
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`

Expected: FAIL because staging and ownership functions do not exist.

- [ ] **Step 3: Implement same-volume staging and immutable input selection.**

Generate `test\input-staging-<guid>` beside managed `test\input-generations`, ensuring source and destination volume roots from `[IO.Path]::GetPathRoot()` are ordinal-ignore-case equal. The exact allowlist is `Invoke-SandboxRunner.ps1`, `lib/Harness.Common.psm1`, all three schemas, selected `scenario.json`, only `scenario-drivers/windows/<scenario-id>.ps1`, and each artifact named by descriptor `arguments`. Validate `test\artifact-manifest.json` as `{schema_version:1,artifacts:[{staged_path,source_relative_path,sha256}]}` with no unknown fields, 1..32 unique entries, production `artifacts/` staged paths, safe relative source paths, and lowercase SHA-256; resolve sources only beneath reparse-safe `test\artifact-sources`. No tests path or artifact source container is staged or mapped. Validate source trees with `Get-SafeRegularFiles`, then copy each allowlisted regular file through create-new streams, creating allowlisted directories once and rejecting reparses before and after every creation.

Build and validate the manifest in staging, write `input-manifest.json` create-new, then use `Get-SafeRegularFiles` to reject anything except manifest-listed files plus the manifest itself. Rename staging on the same volume to unique `test\input-generations\input-<manifest_hash>-<guid>` only if absent. Never delete, replace, or merge an existing directory.

Canonicalize `TestRoot`, reject reparses in it and every existing ancestor, and require exact contained paths `test\input-transition.lock`, `test\input-current.json`, `test\input-selector-history`, and `test\input-generations` before opening anything. If the lock is absent, open it with `FileMode.CreateNew`, read/write access, and `FileShare.None`, then write exact managed bytes `{schema_version:1,kind:"input-transition-lock",lock_id:<32 lowercase hex>}` through that held stream. If present, first reject directory/reparse/non-regular types, then open with `FileMode.Open`, read/write access, and `FileShare.None`; recheck the opened path's regular/non-reparse state and validate exactly those three fields before mutation. A collision blocks; the persistent lock is never deleted.

While holding the lock, require existing `input-current.json` to be regular/non-reparse, open it with `FileMode.Open` and `FileShare.None`, recheck its path while the handle prevents replacement, and validate its bytes exactly as the selector contract below. Resolve its generation by separator-aware containment and revalidate that generation manifest. For a new selection, preflight a unique history destination, rename the old selector to `input-selector-history\input-current-<oldhash>-<guid>.json`, and create the new selector with `Write-JsonCreateNew`; a failure after rename preserves both generations/history and blocks without rollback. Reopen with `FileShare.None` and validate before returning exactly its generation as `InputRoot`. Tests cover malicious lock/selector reparses, wrong lock ownership fields, selector traversal/unknown fields, concurrent publishers, same-hash reuse, selector rename/create failure, prior-name collision, and successful second/third runs without mutation outside managed paths.

```powershell
function Assert-InputTransitionLockRecord { param($Value) $n=@('schema_version','kind','lock_id');Assert-ExactProperties $Value $n $n;if($Value.schema_version-ne 1-or $Value.kind-cne 'input-transition-lock'){throw 'Invalid managed lock constants'};Assert-String $Value.lock_id lock_id '^[0-9a-f]{32}$' }
function Assert-InputSelectorRecord { param($Value) $n=@('schema_version','generation_name','manifest_hash','source_revision','published_at_utc');Assert-ExactProperties $Value $n $n;if($Value.schema_version-ne 1){throw 'Invalid selector version'};Assert-String $Value.generation_name generation_name '^input-[0-9a-f]{64}-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$';Assert-String $Value.manifest_hash manifest_hash '^[0-9a-f]{64}$';Assert-String $Value.source_revision source_revision '^[0-9a-f]{40}$';Assert-UtcTimestamp $Value.published_at_utc published_at_utc }
function Assert-ArtifactManifestRecord { param($Value) $n=@('schema_version','artifacts');Assert-ExactProperties $Value $n $n;if($Value.schema_version -ne 1-or $Value.artifacts -isnot [Array]-or $Value.artifacts.Count -lt 1-or $Value.artifacts.Count -gt 32){throw 'Invalid artifact manifest'};$seen=[Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal);foreach($a in $Value.artifacts){$an=@('staged_path','source_relative_path','sha256');Assert-ExactProperties $a $an $an;Assert-String $a.staged_path staged_path '^artifacts/[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)*$';Assert-String $a.source_relative_path source_relative_path '^[a-z0-9][a-z0-9._-]*(?:/[a-z0-9][a-z0-9._-]*)*$';Assert-String $a.sha256 sha256 '^[0-9a-f]{64}$';if(-not $seen.Add($a.staged_path)){throw 'Duplicate staged artifact'}} }
```

- [ ] **Step 4: Implement exclusive run ownership before children.**

```powershell
function New-HarnessRun {
  param([string]$EvidenceRoot,[string]$RunId)
  Assert-RunId $RunId|Out-Null;Assert-SafeTree (Split-Path $EvidenceRoot -Parent) $EvidenceRoot|Out-Null
  $run=Join-Path $EvidenceRoot $RunId
  $owner=[ordered]@{schema_version=1;run_id=$RunId;owner_nonce=([guid]::NewGuid().ToString('N'));claimed_at_utc=(Get-Date).ToUniversalTime().ToString('o')}
  try{[IO.Directory]::CreateDirectory($run)|Out-Null;Write-JsonCreateNew (Join-Path $run 'owner.json') $owner;Assert-AuthorityRecord owner (Read-JsonFile (Join-Path $run 'owner.json'))}catch{throw "Run ownership collision: $RunId"}
  try{[IO.Directory]::CreateDirectory((Join-Path $run 'control'))|Out-Null;[IO.Directory]::CreateDirectory((Join-Path $run 'sandbox-output'))|Out-Null}catch{throw "Owned run initialization failed: $RunId"}
  [pscustomobject]@{RunId=$RunId;RunRoot=$run;OwnerPath=(Join-Path $run 'owner.json');ControlRoot=(Join-Path $run 'control');OutputRoot=(Join-Path $run 'sandbox-output')}
}
```

The shared `evidence` container is the only location touched before ownership. A loser may participate in idempotent GUID-directory creation, but failure to create and validate `owner.json` is final: it creates no child/control/output or file outside that shared run container. Tests synchronize two jobs at a barrier, repeat 100 times, and assert one valid immutable owner, one loser, no loser writes, unchanged owner bytes, and no output contents.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/fixtures/input scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: stage and claim sandbox runs safely"
```

Expected: PASS for concurrent claims, production-only allowlisting, reparse directories never traversed, malicious locks/selectors, duplicate destinations, copy failure, target collision, prior selector preservation, repeated runs, and exact generation mapping.

### Task 5: Validate Configuration And Record Launch Chronology

**Files:**
- Modify: `scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1`
- Modify: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1`

**Consumes:** validated owner, exact descriptor/profile, selected input generation, templates/hashes, and exact `%SystemRoot%\System32\WindowsSandbox.exe`.
**Produces:** validated WSB, validated create-new `control\launch-intent.json`, then validated create-new `control\process.json` after executable process creation.

- [ ] **Step 1: Write failing tamper and chronology tests.**

```powershell
BeforeEach {
  $input=Join-Path $TestDrive 'input';$control=Join-Path $TestDrive 'run\control';$output=Join-Path $TestDrive 'run\sandbox-output';[IO.Directory]::CreateDirectory($input)|Out-Null;[IO.Directory]::CreateDirectory($control)|Out-Null;[IO.Directory]::CreateDirectory($output)|Out-Null
  $run=[pscustomobject]@{RunId='00000000-0000-0000-0000-000000000001';ControlRoot=$control;OutputRoot=$output};$scenario=[pscustomobject]@{id='clean-install';network_mode='offline'}
  $events=[Collections.Generic.List[string]]::new();$startCalls=0;$fakeStart={param($exe,$arguments)$startCalls++;$events.Add('Start-Process');[pscustomobject]@{Id=4242;StartTime=[datetime]'2026-07-14T00:00:01Z'}}
}
It 'rejects every security-critical template mutation before process start' {
  $template=Join-Path $TestDrive 'tampered.xml';[xml]$xml=Get-Content "$PSScriptRoot/../templates/agent-manager-sandbox-offline.wsb.xml" -Raw;$xml.Configuration.VGpu='Enable';$xml.Save($template)
  {New-Configuration -TemplatePath $template -Scenario $scenario -Profile offline -Run $run -InputRoot $input}|Should -Throw;$startCalls|Should -Be 0
}
It 'writes intent before start and process identity after start' {
  Invoke-OwnedLaunch -Run $run -StartApi $fakeStart
  $events|Should -Be @('launch-intent.json','Start-Process','process.json')
  (Read-JsonFile (Join-Path $run.ControlRoot 'process.json')).sandbox_host_process_id|Should -Be 4242
}
It 'blocks when exact Windows Sandbox executable is unavailable' {
  { Invoke-OwnedLaunch -Run $run -ResolveExecutableApi { $null } -StartApi $fakeStart }|Should -Throw '*blocked*'
  $startCalls|Should -Be 0;Assert-AuthorityRecord failure (Read-JsonFile (Join-Path $run.ControlRoot 'launch-failure.json'))
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`

Expected: FAIL because configuration and launch functions do not exist.

- [ ] **Step 3: Generate, reload, and validate every configuration node.**

`New-Configuration` loads the exact route `templates\agent-manager-sandbox-$Profile.wsb.xml`, populates DOM text nodes, saves create-new under `control`, reloads that exact saved path, and rejects unless all conditions hold: root is exactly `Configuration`; `VGpu`, `ClipboardRedirection`, `PrinterRedirection`, `AudioInput`, and `VideoInput` are exactly `Disable`; `Networking` is exactly `Disable` offline or `Enable` online; `LogonCommand.Command` exactly equals the constructed runner command; there is exactly one `MappedFolders` node and exactly two `MappedFolder` children; map 0 has canonical exact host `InputRoot`, Sandbox path `C:\AgentManagerHarness\Input`, and `ReadOnly` text `true`; map 1 has canonical exact host `Run.OutputRoot`, Sandbox path `C:\AgentManagerHarness\Evidence`, and `ReadOnly` text `false`; no other element or attribute exists; neither run root nor control root is mapped; both host paths are non-overlapping and reparse-free.

Tests independently tamper each device, networking, command, root, attribute, element, map count/order, host path, Sandbox path, `ReadOnly`, extra map, parent map, and child map. Each case asserts the injected process API was never called.

- [ ] **Step 4: Implement immutable two-record launch provenance.**

Resolve exactly `[Environment]::ExpandEnvironmentVariables('%SystemRoot%\System32\WindowsSandbox.exe')`, canonicalize it, require equality to that expanded path, and reject absent, directory, or reparse entries. No PATH search, App Paths lookup, association-based launch, or WSB-file invocation is allowed. Unavailability writes create-new `launch-failure-details.json`, hashes it, then writes validated create-new `launch-failure.json` with stage `resolve-windows-sandbox`, status `blocked`, and calls no process API.

Before launch, create and validate `control\launch-intent.json` with the exact authority fields, including canonical `windows_sandbox_executable` and `wsb_path`, and no PID. Invoke only the injected process API whose production implementation is `Start-Process -FilePath $windowsSandboxExe -ArgumentList @($wsbPath) -PassThru`; static tests supply a fake and never launch the executable. Immediately after return, create and validate `control\process.json` from the returned executable PID and start time. At timeout, reload that exact PID and compare start time before stopping; absence means already exited, mismatch means PID reuse and must not be stopped, and either condition is recorded without a broad kill. If process-record creation fails, identity-check the returned process before stop, write validated create-new failure authority, and block collection.

The collector consumes both immutable records. Runner completion copies explicit run/configuration/input identity passed in the logon command, while the host validates it against unmapped authority. Tests prove intent unknown/missing/wrong-type fields fail, intent collision prevents launch, PID is absent from intent, process record cannot precede start, WindowsSandbox unavailability is blocked, process collision triggers identity-checked stop, PID reuse is never stopped, and no mutable record is used.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: validate and record sandbox launch"
```

### Task 6: Validate Production Drivers And Clean Owned Process Trees

**Files:**
- Create: `scripts/windows-sandbox/Invoke-SandboxRunner.ps1`
- Create: `scripts/windows-sandbox/tests/fixtures/evidence/required.txt`
- Create: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`
- Test: `scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1`

**Consumes:** strict production descriptor/manifest, read-only input generation, explicit run identity/profile/hashes, and output mapping.
**Produces:** bounded future-only production driver execution, immutable driver identity/cleanup records, identity-aware descendant cleanup, failure evidence, and no broad kill.

- [ ] **Step 1: Write failing command-containment and process-tree tests.**

```powershell
BeforeEach {
  $input=Join-Path $TestDrive 'input';[IO.Directory]::CreateDirectory($input)|Out-Null;$startCalls=0
  $manifest=[pscustomobject]@{files=@([pscustomobject]@{path='scenario-drivers/windows/clean-install.ps1';length=1;sha256=('a'*64)})}
  $root=[pscustomobject]@{process_id=11;parent_process_id=1;creation_time_utc='2026-07-14T00:00:01.0000000Z'};$deadline=[datetime]::UtcNow.AddSeconds(5);$calls=[Collections.Generic.List[int]]::new()
  $api=@{GetChildren={param($identity)if($identity.process_id-eq 11){@(12)}elseif($identity.process_id-eq 12){@(13)}else{@()}};GetIdentity={param($id)[pscustomobject]@{process_id=$id;parent_process_id=if($id-eq 11){1}else{$id-1};creation_time_utc="2026-07-14T00:00:0$($id-10).0000000Z"}};Stop={param($identity)$calls.Add([int]$identity.process_id)}}
}
It 'rejects traversal, absolute, unlisted, fixture, and wrong production drivers without launch' {
  foreach($p in '..\evil.exe','C:\Windows\System32\cmd.exe','missing.ps1','Invoke-SandboxRunner.ps1'){ {Resolve-DeclaredProcess -InputRoot $input -Manifest $manifest -DeclaredProcess $p}|Should -Throw };$startCalls|Should -Be 0
}
It 'stops descendants before parents by traversal depth and detects PID reuse' {
  $result=Stop-OwnedProcessTree -RootIdentity $root -Api $api -DeadlineUtc $deadline
  $calls|Should -Be @(13,12,11);$result.cleanup_status|Should -Be 'succeeded';$calls|Should -Not -Contain 99
}
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1 -Output Detailed`

Expected: FAIL because runner functions do not exist.

- [ ] **Step 3: Implement descriptor/process validation before launch.**

`Assert-RunnerPreflight` validates both contracts, recomputes manifest payload bytes and every listed file hash, enumerates via `Get-SafeRegularFiles`, rejects extras/reparses, validates profile equality, and resolves `declared_process` beneath `C:\AgentManagerHarness\Input`. Its normalized path must exactly match one manifest entry and equal `scenario-drivers/windows/<scenario_id>.ps1`; any `tests/`, runner, arbitrary script, or different scenario path fails. Resolve every `arguments` entry through the same containment/reparse procedure and require an exact manifest entry under `artifacts/`. Pass canonical artifact paths, evidence root, and the runner-owned API adapter to fixed trusted `powershell.exe -NoProfile -ExecutionPolicy Bypass -File <production driver>`. No descriptor value becomes free-form command text. Static Pester uses fake process/driver APIs and does not execute production drivers, Sandbox, product, installer, or OS commands.

- [ ] **Step 4: Implement bounded, identity-aware post-order cleanup.**

Immediately after starting the driver, write and validate create-new `runner-process.json` containing exact driver path, PID, creation time, and start timestamps. Represent cleanup identity as `{process_id,parent_process_id,creation_time_utc}` from CIM. Starting at the returned root PID/start time, repeatedly snapshot descendants whose current parent identity matches the recorded parent and whose creation time is not earlier than the parent. Build `(identity, depth)` nodes by traversal, sort by depth descending using `[Array]::Sort` with a comparison delegate, and stop descendants before parents. Immediately before each stop, reload PID, parent PID, and creation time; mismatch records `pid-reused` and is never stopped.

After each deepest-first pass, rescan surviving owned parents for newly appearing children until two consecutive empty scans or the cleanup deadline. Wait only within the bounded deadline. Write and validate create-new `cleanup.json` with the same driver PID/creation time, exit code, cleanup status, exact failed identities/reasons, and end time. Any cleanup error prevents `complete.json` and never falls back to image/product-name termination. Pester fake snapshots model 11 -> 12 -> 13, late child 14, reused PID 12, access denial, disappearing nodes, and unrelated PID 99, and assert traversal order rather than numeric PID order.

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

**Consumes:** required evidence, driver identity/cleanup result, explicit run ID, validated unmapped owner/intent/host-process/manifest/configuration records, and one output child.
**Produces:** create-new `complete.json` written last by runner and create-new `control\collection.json` written after full host recomputation.

- [ ] **Step 1: Write failing finalization and collector tests.**

```powershell
BeforeEach {
  $id='00000000-0000-0000-0000-000000000001';$evidence=Join-Path $TestDrive 'evidence';$runRoot=Join-Path $evidence $id;$control=Join-Path $runRoot 'control';$root=Join-Path $runRoot 'sandbox-output';[IO.Directory]::CreateDirectory($control)|Out-Null;[IO.Directory]::CreateDirectory($root)|Out-Null
  [IO.File]::WriteAllText((Join-Path $root 'results.tsv'),'ok');$identity=[pscustomobject]@{run_id=$id;scenario_id='clean-install';profile='offline';input_manifest_hash=('a'*64);configuration_hash=('b'*64);source_revision=('c'*40);driver_process_id=42;driver_process_start_time_utc='2026-07-14T00:00:02.0000000Z';started_at_utc='2026-07-14T00:00:03.0000000Z'}
}
It 'does not complete after cleanup failure or with an extra file' { {Finalize-Evidence -EvidenceRoot $root -CleanupStatus failed -RequiredEvidence @('results.tsv') -Identity $identity}|Should -Throw;Test-Path (Join-Path $root 'complete.json')|Should -BeFalse }
It 'rejects changed current evidence and impossible chronology' { {Assert-CollectedRun -RunId $id -EvidenceRoot $evidence}|Should -Throw '*hash*';Test-Path (Join-Path $control 'collection.json')|Should -BeFalse }
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1, .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed`

Expected: FAIL because complete collection is not implemented.

- [ ] **Step 3: Implement runner complete-last semantics.**

Allowed output before completion is exactly descriptor `required_evidence` plus `started.json`, validated `runner-process.json`, and validated `cleanup.json`; validated `failure.json` is allowed only on a failed run and precludes completion. Re-enumerate only with `Get-SafeRegularFiles`, reject every directory reparse and any extra/unlisted file, require nonempty evidence, and recalculate all hashes. Require driver PID/creation identity to match across runner-process, cleanup, and complete; validate `driver_process_start <= started <= cleanup ended <= finalized`; require cleanup `succeeded` and explicit run identity. Construct the strict complete object, validate it, then write `complete.json` create-new as the final filesystem operation. No cleanup or evidence mutation follows it; driver-unit success alone does not complete a scenario.

- [ ] **Step 4: Implement collector full recomputation from explicit identity.**

`Get-RunControlRecord` accepts only caller `RunId`, validates it, derives exactly `EvidenceRoot\RunId`, validates `owner.json` with the owner contract and matching RunId, and derives only `control` and `sandbox-output`; it never enumerates `EvidenceRoot`. The collector validates intent, host-process, runner-process, cleanup, manifest, and complete contracts; recomputes configuration/manifest/file hashes; compares every shared identity/hash/profile/revision field; requires driver PID and creation time to match immutable runner/cleanup/complete values; and validates `claim <= intent <= sandbox_host_process_start <= driver_process_start <= started <= cleanup ended <= finalized <= collection` with at most 120 seconds host/Sandbox skew.

Collection never requires the terminated driver PID to remain present. If that numeric PID now resolves to a different creation time, it is ordinary PID reuse, not acceptance evidence and not a process to stop; acceptance relies on immutable recorded identity, successful bounded cleanup, chronology, and current hashes. Enumerate output only with `Get-SafeRegularFiles`; reject every reparse directory before descent, missing/extra/changed file, or cleanup failure, and recalculate every current evidence hash.

Only after all checks pass, construct, validate, and write create-new `control\collection.json` with the exact collection authority fields. A failed collection constructs and validates create-new `collection-failure.json`, never writes collection authority, never changes output, and preserves all records. `control\archive.json` is an optional create-new index after accepted collection and contains the same explicit run ID/current hashes; it copies and extracts nothing.

- [ ] **Step 5: Run tests, verify green, and commit.**

```powershell
Invoke-Pester .\scripts\windows-sandbox\tests\Runner.Evidence.Tests.ps1, .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1 -Output Detailed
git add scripts/windows-sandbox/Invoke-SandboxRunner.ps1 scripts/windows-sandbox/Invoke-AgentManagerSandbox.ps1 scripts/windows-sandbox/tests/Runner.Evidence.Tests.ps1 scripts/windows-sandbox/tests/Launcher.Preflight.Tests.ps1
git commit -m "feat: finalize and collect sandbox evidence"
```

Expected: PASS for reparse directories never traversed, extra/missing/changed files, malformed authority contracts, run/driver identity mismatch, timestamp inversion/skew, owner mismatch, PID reuse after termination, configuration/manifest recomputation, cleanup failure, complete collision, and explicit-run-only collection.

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
BeforeEach {
  $id='00000000-0000-0000-0000-000000000001';$root=Join-Path $TestDrive 'test';$control=Join-Path $TestDrive 'control';[IO.Directory]::CreateDirectory($root)|Out-Null;[IO.Directory]::CreateDirectory($control)|Out-Null
  $wsb=Join-Path $root 'agent-manager-test.wsb';$runner=Join-Path $root 'run-agent-manager-sandbox.ps1';[IO.File]::WriteAllText($wsb,'legacy');[IO.File]::WriteAllText($runner,'legacy');$launchCalls=0;$moves=0;$failSecond={param($source,$destination)$moves++;if($moves-eq 2){throw 'injected second move failure'};[IO.File]::Move($source,$destination)}
}
It 'moves neither source when any destination exists' { $retired=Join-Path $root "retired\$id";[IO.Directory]::CreateDirectory($retired)|Out-Null;[IO.File]::WriteAllText((Join-Path $retired 'agent-manager-test.wsb'),'collision');{Move-LegacyHarnessToQuarantine -TestRoot $root -ControlRoot $control -RunId $id}|Should -Throw;Test-Path $wsb|Should -BeTrue;Test-Path $runner|Should -BeTrue }
It 'records a partial move as blocked without rollback or launch' { {Move-LegacyHarnessToQuarantine -TestRoot $root -ControlRoot $control -RunId $id -MoveApi $failSecond}|Should -Throw;(Read-JsonFile (Join-Path $control 'legacy-quarantine-failure.json')).status|Should -Be 'blocked';$launchCalls|Should -Be 0 }
```

- [ ] **Step 2: Run tests and verify red.**

Run: `Invoke-Pester .\scripts\windows-sandbox\tests\Launcher.Preflight.Tests.ps1, .\scripts\windows-sandbox\tests\Scenario.Matrix.Tests.ps1 -Output Detailed`

Expected: FAIL because quarantine and documentation do not exist.

- [ ] **Step 3: Implement fail-closed quarantine and provenance.**

Preflight both exact sources `agent-manager-test.wsb` and `run-agent-manager-sandbox.ps1` and both exact destinations under `test\retired\<RunId>` before either move: validate test/control/retired containment and every ancestor, reject reparses, require both regular sources, require destination directory newly created and empty, and require both targets absent. Hash both sources and write `control\legacy-inventory.json` create-new before moves. Move source one then source two without overwrite. After each move, verify destination hash and source absence. Write `legacy-quarantine.json` create-new only after both succeed.

If either move or verification fails, write per-source states to create-new `legacy-quarantine-failure-details.json`, hash it, then write validated create-new `legacy-quarantine-failure.json` with stage `legacy-quarantine`, `status: blocked`, and matching `details_hash`; preserve partial source/destination state, perform no rollback, delete nothing, launch nothing, and require operator review. A preflight failure moves neither source. Tests inject first/second failures, destination collision, source disappearance, reparse, hash change, and provenance collision.

- [ ] **Step 4: Document future-only Windows commands and honest macOS boundary.**

README labels all ten launcher commands **FUTURE EXECUTION ONLY - DO NOT RUN DURING IMPLEMENTATION**, uses the exact authorized worktree, routes only `proxy-failure` online and the other nine offline, and maps each descriptor to its production `scenario-drivers/windows/<id>.ps1`. Before those commands, an operator must place artifacts only under `D:\codex\ai-deploy-toolkit\test\artifact-sources` and create strict `D:\codex\ai-deploy-toolkit\test\artifact-manifest.json` with verified hashes; the source container is never mapped. It states drivers may invoke staged product/MSI/WebView/owned-process actions only inside a future Sandbox after all host and runner preflight succeeds; implementation tests inspect routes and inject APIs but never execute drivers, WindowsSandbox.exe, product, or installers. It requires explicit returned run IDs for collection and forbids searching peer evidence directories. It documents rollback as selecting a validated tracked revision in an isolated checkout and creating a new input generation/run, never restoring the broad writable mapping. It states all 18 macOS cases require respective real disposable Intel/Apple Silicon hardware and static fixture/hash/ZIP/Mach-O/source observations do not satisfy interaction acceptance.

The README includes these exact gated commands:

```powershell
Set-Location D:\codex\ai-deploy-toolkit\apps\agent-manager\.worktrees\installation-orchestrator
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId clean-install -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId uac-accept -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId uac-decline -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId path-refresh -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId multiple-node-installations -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId proxy-failure -Profile online -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId file-lock -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId disk-space-guard -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId batch-partial-failure -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
.\scripts\windows-sandbox\Invoke-AgentManagerSandbox.ps1 -ScenarioId postflight-path-version -Profile offline -TestRoot D:\codex\ai-deploy-toolkit\test -ArtifactManifestPath D:\codex\ai-deploy-toolkit\test\artifact-manifest.json
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

Expected: PASS using only harmless fixtures, file inspection, `TestDrive`, and mocked APIs; no production driver, WindowsSandbox.exe, product, MSI, WebView, or installer is executed. Record exact commit/results without calling static success interactive acceptance.

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

- Alternative B remains unchanged and the locked tree now includes exactly ten production Windows drivers: immutable selected input generation, unmapped control, and one fresh writable output.
- Run ownership is a strict create-new `owner.json` before child creation; concurrency has one owner, losers fail closed, and all authority records have exact validators and immutable field sets.
- Launch chronology resolves exact `WindowsSandbox.exe`, uses immutable intent before executable launch and immutable process identity after launch, and identity-checks timeout stops.
- Manifest ordering is ordinal and its exact BOM-free payload bytes exclude `manifest_hash`; host and Sandbox reconstruct identical bytes.
- Runtime enforcement uses repository-owned explicit validators matching complete documentation schemas, with adversarial coverage of fields, types, patterns, arrays, uniqueness, and bounds.
- Configuration reload validates every exact security node, command, mapping, `ReadOnly` value, and absence of extras before process start.
- The explicit non-following walker rejects reparse files/directories before descent and is shared by manifest creation, staging validation, runner preflight, and collection.
- Staging is same-volume, production-allowlisted, lock/selector-validated, immutable, failure-preserving, and never deletes or overwrites prior input generations.
- Cleanup is identity-aware post-order traversal with bounded rescans, PID-reuse defense, no broad kill, and completion blocked on any failure.
- Production drivers and artifact arguments are manifest-contained inputs under exact production paths; harmless fixtures remain Pester-only, and static tests cannot launch Sandbox or product commands.
- Driver start/cleanup records retain PID plus creation identity; collection compares immutable identity/chronology without requiring a terminated PID to exist or accepting a reused PID.
- Finalization and collection re-enumerate safely and rehash all current evidence, validate chronology/run identity, reject extras, and write completion last only after cleanup success.
- Legacy quarantine preflights both sources/destinations, uses create-new provenance, blocks partial moves without rollback, and never launches or deletes legacy files.
- Review every snippet for Windows PowerShell 5.1 syntax/API availability, defined helpers, TOCTOU boundaries, path-root edges, reparse traversal, safe move behavior, matching field names, and honest expected results.

Plan complete and saved to `docs/superpowers/plans/2026-07-14-windows-sandbox-harness-hardening.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task and review each task before the next.
2. Inline Execution - execute tasks in this session using executing-plans with review checkpoints.
