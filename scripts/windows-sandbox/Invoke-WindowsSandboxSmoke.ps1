[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$MsiPath,
    [Parameter(Mandatory = $true)][string]$PortableZipPath,
    [Parameter(Mandatory = $true)][string]$ChecksumsPath,
    [string]$WebView2Path,
    [string]$EvidenceRoot = (Join-Path ([IO.Path]::GetTempPath()) 'AgentManagerSandboxSmoke'),
    [switch]$PrepareOnly,
    [switch]$Launch
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

function Get-FullPath([string]$Path) { return [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $Path).Path) }
function Assert-File([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw "Missing file: $Path" }
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point rejected: $Path" }
    return (Get-FullPath $Path)
}
function Assert-NonOverlapping([string]$A, [string]$B) {
    $x = $A.TrimEnd('\') + '\'; $y = $B.TrimEnd('\') + '\'
    if ($x.Equals($y, [StringComparison]::OrdinalIgnoreCase) -or $x.StartsWith($y, [StringComparison]::OrdinalIgnoreCase) -or $y.StartsWith($x, [StringComparison]::OrdinalIgnoreCase)) { throw 'Mapped paths overlap.' }
}
function Assert-NoReparse([string]$Path) {
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw "Reparse point rejected: $Path" }
}
function Write-CreateNewJson([string]$Path, $Value) {
    $json = $Value | ConvertTo-Json -Depth 8
    $file = New-Object IO.FileStream($Path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $stream = New-Object IO.StreamWriter($file, (New-Object Text.UTF8Encoding($false)))
    try { $stream.Write($json) } finally { $stream.Dispose(); $file.Dispose() }
}
function Assert-Checksum([string]$Checksums, [string]$File) {
    $name = [IO.Path]::GetFileName($File)
    $line = Get-Content -LiteralPath $Checksums | Where-Object { $_ -match "\s$name$" } | Select-Object -First 1
    if (-not $line) { throw "Checksum entry missing: $name" }
    $expected = ($line -split '\s+')[0].ToLowerInvariant()
    $actual = (Get-FileHash -LiteralPath $File -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($expected -ne $actual) { throw "Checksum mismatch: $name" }
}
function Get-ManifestPayloadJson($Manifest) {
    $payload = [ordered]@{
        schema = [int]$Manifest.schema
        run_id = [string]$Manifest.run_id
        scenario = [string]$Manifest.scenario
        network_mode = [string]$Manifest.network_mode
        files = @($Manifest.files | Sort-Object { [string]$_.path } | ForEach-Object {
                [ordered]@{ path = [string]$_.path; length = [int64]$_.length; sha256 = [string]$_.sha256 }
            })
    }
    return ($payload | ConvertTo-Json -Depth 8 -Compress)
}

if ($PrepareOnly -and $Launch) { throw 'Choose -PrepareOnly or -Launch, not both.' }
$msi = Assert-File $MsiPath; $portable = Assert-File $PortableZipPath; $sums = Assert-File $ChecksumsPath
if ($WebView2Path) { $webview = Assert-File $WebView2Path } else { $webview = $null }
Assert-Checksum $sums $msi
Assert-Checksum $sums $portable
$root = [IO.Path]::GetFullPath($EvidenceRoot)
New-Item -ItemType Directory -Path $root -Force | Out-Null
Assert-NoReparse $root
$runId = [guid]::NewGuid().ToString('N')
$run = Join-Path $root $runId; $control = Join-Path $run 'control'; $input = Join-Path $run 'input'; $output = Join-Path $run 'sandbox-output'
New-Item -ItemType Directory -Path $control,$input,$output | Out-Null
Assert-NonOverlapping $input $output
if (@(Get-ChildItem -LiteralPath $output -Force).Count -ne 0) { throw 'Output directory must be empty.' }

$files = @(
    [ordered]@{ source = $msi; name = 'Agent-Manager.msi' },
    [ordered]@{ source = $portable; name = 'Agent-Manager-Portable.zip' },
    [ordered]@{ source = $sums; name = 'SHA256SUMS.txt' }
)
if ($webview) { $files += [ordered]@{ source = $webview; name = 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe' } }
foreach ($file in $files) { Copy-Item -LiteralPath $file.source -Destination (Join-Path $input $file.name) -Force }
$runner = Join-Path $input 'Run-WindowsSandboxSmoke.ps1'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Run-WindowsSandboxSmoke.ps1') -Destination $runner
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'ManifestContract.ps1') -Destination (Join-Path $input 'ManifestContract.ps1')
$manifestEntries = @()
foreach ($file in (Get-ChildItem -LiteralPath $input -File | Sort-Object Name)) {
    $manifestEntries += [ordered]@{ path = $file.Name; length = $file.Length; sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() }
}
$manifest = [ordered]@{ schema = 1; run_id = $runId; scenario = 'package-smoke'; network_mode = 'offline'; files = @($manifestEntries) }
$manifestJson = Get-ManifestPayloadJson $manifest
$manifestHash = [BitConverter]::ToString(([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($manifestJson)))).Replace('-','').ToLowerInvariant()
$manifest.manifest_sha256 = $manifestHash
Write-CreateNewJson (Join-Path $control 'manifest.json') $manifest
Write-CreateNewJson (Join-Path $input 'manifest.json') $manifest

$wsbPath = Join-Path $control 'package-smoke.wsb'
$wsb = @"
<Configuration>
  <Networking>Disable</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <PrinterRedirection>Disable</PrinterRedirection>
  <AudioInput>Disable</AudioInput>
  <VideoInput>Disable</VideoInput>
  <VGpu>Disable</VGpu>
  <MappedFolders>
    <MappedFolder><HostFolder>$([Security.SecurityElement]::Escape($input))</HostFolder><SandboxFolder>C:\AgentManagerHarness\Input</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>$([Security.SecurityElement]::Escape($output))</HostFolder><SandboxFolder>C:\AgentManagerHarness\Evidence</SandboxFolder><ReadOnly>false</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\AgentManagerHarness\Input\Run-WindowsSandboxSmoke.ps1 -RunId $runId</Command></LogonCommand>
</Configuration>
"@
[IO.File]::WriteAllText($wsbPath, $wsb, (New-Object Text.UTF8Encoding($false)))
$configHash = (Get-FileHash -LiteralPath $wsbPath -Algorithm SHA256).Hash.ToLowerInvariant()
Write-CreateNewJson (Join-Path $control 'launch.json') ([ordered]@{ schema = 1; run_id = $runId; scenario = 'package-smoke'; profile = 'offline'; manifest_sha256 = $manifestHash; config_sha256 = $configHash; input = $input; output = $output; status = 'prepared' })
Write-CreateNewJson (Join-Path $control 'result.json') ([ordered]@{ status = 'prepared'; run_id = $runId; launch_required = $true; evidence = $run })
Write-Output "Prepared run $runId"
Write-Output "Config: $wsbPath"
if ($Launch) {
    if (-not (Get-Command WindowsSandbox.exe -ErrorAction SilentlyContinue)) { throw 'WindowsSandbox.exe is not available.' }
    & WindowsSandbox.exe $wsbPath
    if ($LASTEXITCODE -ne 0) { throw "Windows Sandbox failed with exit code $LASTEXITCODE." }
}
