[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$MsiPath,
    [Parameter(Mandatory = $true)][string]$ChecksumsPath,
    [Parameter(Mandatory = $true)][string]$WebView2Path,
    [string]$RuntimeRoot = 'D:\codex\ai-deploy-toolkit\test\windows-sandbox',
    [switch]$PrepareOnly,
    [switch]$Launch
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'ManifestContract.ps1')

function Assert-File([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing file: $Path" }
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point rejected: $Path" }
    return [IO.Path]::GetFullPath($item.FullName)
}
function Write-CreateNewJson([string]$Path, $Value) {
    $file = New-Object IO.FileStream($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $stream = New-Object IO.StreamWriter($file, (New-Object Text.UTF8Encoding($false)))
    try { $stream.Write(($Value | ConvertTo-Json -Depth 8)) } finally { $stream.Dispose(); $file.Dispose() }
}
function Assert-MsiChecksum([string]$Checksums, [string]$Msi) {
    $name = [IO.Path]::GetFileName($Msi)
    $line = Get-Content -LiteralPath $Checksums | Where-Object { $_ -match "\s$name$" } | Select-Object -First 1
    if (-not $line) { throw "Checksum entry missing: $name" }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $Msi -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expected -ne $actual) { throw "Checksum mismatch: $name" }
}

if ($PrepareOnly -and $Launch) { throw 'Choose -PrepareOnly or -Launch, not both.' }
$systemDrive = Get-PSDrive -Name C
if ($systemDrive.Free -lt 20GB) { throw 'Windows Sandbox launch refused because C: has less than 20 GiB free.' }
if ($Launch -and @(Get-Process -Name 'WindowsSandbox','WindowsSandboxClient' -ErrorAction SilentlyContinue).Count -ne 0) { throw 'Another Windows Sandbox scenario is already running.' }
$msi = Assert-File $MsiPath
$checksums = Assert-File $ChecksumsPath
$webview = Assert-File $WebView2Path
Assert-MsiChecksum $checksums $msi

$root = [IO.Path]::GetFullPath($RuntimeRoot)
$runsRoot = Join-Path $root 'runs'
New-Item -ItemType Directory -Path $runsRoot -Force | Out-Null
if ((Get-Item -LiteralPath $root -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Sandbox runtime root cannot be a reparse point.' }
$runId = [guid]::NewGuid().ToString('N')
$run = Join-Path $runsRoot $runId
$control = Join-Path $run 'control'
$input = Join-Path $run 'input'
$output = Join-Path $run 'sandbox-output'
New-Item -ItemType Directory -Path $control,$input,$output | Out-Null

$staged = @(
    [ordered]@{ source = $msi; name = 'Agent-Manager.msi' },
    [ordered]@{ source = $checksums; name = 'SHA256SUMS.txt' },
    [ordered]@{ source = $webview; name = 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe' },
    [ordered]@{ source = (Join-Path $PSScriptRoot 'Run-WindowsSandboxGuidedInstall.ps1'); name = 'Run-WindowsSandboxGuidedInstall.ps1' },
    [ordered]@{ source = (Join-Path $PSScriptRoot 'Collect-WindowsSandboxGuidedInstall.ps1'); name = 'Collect-WindowsSandboxGuidedInstall.ps1' },
    [ordered]@{ source = (Join-Path $PSScriptRoot 'ManifestContract.ps1'); name = 'ManifestContract.ps1' }
)
foreach ($file in $staged) { Copy-Item -LiteralPath (Assert-File $file.source) -Destination (Join-Path $input $file.name) }
foreach ($item in Get-ChildItem -LiteralPath $input -File) { $item.IsReadOnly = $true }

$entries = @()
foreach ($file in Get-ChildItem -LiteralPath $input -File) {
    $entries += [ordered]@{ path = $file.Name; length = $file.Length; sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
}
$manifest = [ordered]@{ schema = 1; run_id = $runId; scenario = 'guided-install'; network_mode = 'online'; files = @($entries) }
$payloadJson = Get-ManifestPayloadJson $manifest
$manifestHash = [BitConverter]::ToString(([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($payloadJson)))).Replace('-','').ToLowerInvariant()
$manifest.manifest_sha256 = $manifestHash
Write-CreateNewJson (Join-Path $control 'manifest.json') $manifest
Write-CreateNewJson (Join-Path $input 'manifest.json') $manifest
(Get-Item -LiteralPath (Join-Path $input 'manifest.json')).IsReadOnly = $true

$wsbPath = Join-Path $control 'guided-install.wsb'
$wsb = @"
<Configuration>
  <Networking>Default</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <PrinterRedirection>Disable</PrinterRedirection>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <VGpu>Disable</VGpu>
  <MemoryInMB>2048</MemoryInMB>
  <MappedFolders>
    <MappedFolder><HostFolder>$([Security.SecurityElement]::Escape($input))</HostFolder><SandboxFolder>C:\AgentManagerHarness\Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$([Security.SecurityElement]::Escape($output))</HostFolder><SandboxFolder>C:\AgentManagerHarness\Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\AgentManagerHarness\Input\Run-WindowsSandboxGuidedInstall.ps1 -RunId $runId</Command></LogonCommand>
</Configuration>
"@
[IO.File]::WriteAllText($wsbPath, $wsb, (New-Object Text.UTF8Encoding($false)))
$configHash = (Get-FileHash -LiteralPath $wsbPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-CreateNewJson (Join-Path $control 'launch.json') ([ordered]@{ schema = 1; run_id = $runId; scenario = 'guided-install'; profile = 'online'; manifest_sha256 = $manifestHash; config_sha256 = $configHash; status = 'prepared' })
Write-Output "Prepared guided run $runId"
Write-Output "Config: $wsbPath"
if ($Launch) {
    $process = Start-Process -FilePath $wsbPath -PassThru
    Write-Output "Launched Windows Sandbox process $($process.Id)"
}
