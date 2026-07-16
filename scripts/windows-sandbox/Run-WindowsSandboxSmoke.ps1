[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$RunId)
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$inputRoot = 'C:\AgentManagerHarness\Input'; $evidence = 'C:\AgentManagerHarness\Evidence'
. (Join-Path $PSScriptRoot 'ManifestContract.ps1')
function New-Record([string]$Name, $Value) {
    $path = Join-Path $evidence $Name
    $file = New-Object IO.FileStream($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $stream = New-Object IO.StreamWriter($file, (New-Object Text.UTF8Encoding($false)))
    try { $stream.Write(($Value | ConvertTo-Json -Depth 8)) } finally { $stream.Dispose(); $file.Dispose() }
}
function Invoke-Step([string]$Name, [scriptblock]$Action) {
    $started = [DateTime]::UtcNow
    try { & $Action; return [ordered]@{ name = $Name; status = 'passed'; started_utc = $started.ToString('o'); ended_utc = [DateTime]::UtcNow.ToString('o') } }
    catch { return [ordered]@{ name = $Name; status = 'failed'; error = $_.Exception.Message; started_utc = $started.ToString('o'); ended_utc = [DateTime]::UtcNow.ToString('o') } }
}
function Quote-ProcessArgument([string]$Value) { return '"' + $Value.Replace('"', '\"') + '"' }
function Get-ShortcutTarget {
    $shell = New-Object -ComObject WScript.Shell
    $roots = @(
        (Join-Path $env:ProgramData 'Microsoft\Windows\Start Menu\Programs'),
        (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs')
    )
    foreach ($root in $roots) {
        if (-not (Test-Path -LiteralPath $root)) { continue }
        foreach ($shortcut in (Get-ChildItem -LiteralPath $root -Filter '*Agent Manager*.lnk' -Recurse -File)) {
            $target = $shell.CreateShortcut($shortcut.FullName).TargetPath
            if ($target -and (Test-Path -LiteralPath $target)) { return $target }
        }
    }
    return $null
}
function Save-Screenshot([string]$Path) {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing
    $bounds = [Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bitmap = New-Object Drawing.Bitmap($bounds.Width, $bounds.Height)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try { $graphics.CopyFromScreen($bounds.Location, [Drawing.Point]::Empty, $bounds.Size) } finally { $graphics.Dispose() }
        $bitmap.Save($Path, [Drawing.Imaging.ImageFormat]::Png)
    } finally { $bitmap.Dispose() }
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { throw 'Screenshot was not created.' }
}
function Complete-Run([array]$Steps, [string]$Status, [string]$ManifestHash) {
    New-Record 'steps.json' $Steps
    New-Record 'complete.json' ([ordered]@{
            schema = 1
            run_id = $RunId
            scenario = 'package-smoke'
            profile = 'offline'
            manifest_sha256 = $ManifestHash
            status = $Status
            completed_utc = [DateTime]::UtcNow.ToString('o')
            steps = $Steps
    })
}
$manifest = Get-Content -LiteralPath (Join-Path $inputRoot 'manifest.json') -Raw | ConvertFrom-Json
$manifestHash = [string]$manifest.manifest_sha256
if ($manifestHash -notmatch '^[0-9a-f]{64}$') { throw 'Manifest hash is not a lowercase SHA-256 value.' }
if ($manifest.run_id -ne $RunId -or $manifest.scenario -ne 'package-smoke' -or $manifest.network_mode -ne 'offline') { throw 'Manifest identity or offline contract mismatch.' }
$manifestEntries = @($manifest.files)
if (-not $manifest.PSObject.Properties['files'] -or $manifestEntries.Count -eq 0) { throw 'Manifest file list is missing.' }
$seenPaths = @{}
foreach ($entry in $manifestEntries) {
    $path = [string]$entry.path
    if ([string]::IsNullOrWhiteSpace($path) -or [IO.Path]::IsPathRooted($path) -or $path.Contains('\') -or $path.Contains('/') -or $path -in @('.', '..')) { throw "Manifest path is not a leaf filename: $path" }
    $key = $path.ToLowerInvariant()
    if ($seenPaths.ContainsKey($key)) { throw "Duplicate manifest path: $path" }
    $seenPaths[$key] = $true
    if ([string]$entry.sha256 -notmatch '^[0-9a-f]{64}$') { throw "Manifest hash is malformed: $path" }
    $declaredLength = Convert-ManifestLength $entry.length
    if ($null -eq $declaredLength) { throw "Manifest length is not a nonnegative Int64 integer: $path" }
    $actualPath = Join-Path $inputRoot $path
    if (-not (Test-Path -LiteralPath $actualPath -PathType Leaf)) { throw "Manifest file is missing: $path" }
    $actualFile = Get-Item -LiteralPath $actualPath -Force
    if ($actualFile.Length -ne $declaredLength) { throw "Manifest length mismatch: $path" }
    if ((Get-FileHash -LiteralPath $actualPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "Manifest file hash mismatch: $path" }
}
$recomputedManifestHash = [BitConverter]::ToString(([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes((Get-ManifestPayloadJson $manifest))))).Replace('-','').ToLowerInvariant()
if ($recomputedManifestHash -ne $manifestHash) { throw 'Manifest payload hash mismatch.' }
New-Record 'started.json' ([ordered]@{ schema = 1; run_id = $RunId; scenario = 'package-smoke'; profile = 'offline'; started_utc = [DateTime]::UtcNow.ToString('o') })
$steps = @()
$steps += Invoke-Step 'artifact-and-hash-contract' { foreach ($entry in $manifest.files) { if ((Get-FileHash (Join-Path $inputRoot $entry.path) -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "Hash mismatch: $($entry.path)" } } }
$artifactGate = $steps[-1]
if ($artifactGate.status -ne 'passed') { Complete-Run $steps 'failed' $manifestHash; exit 1 }
$steps += Invoke-Step 'offline-webview2-prerequisite' {
    $installer = Join-Path $inputRoot 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
    $guid = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    $runtimeVersion = @(
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid",
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$guid",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$guid"
    ) | ForEach-Object {
        if (Test-Path -LiteralPath $_) {
            $version = (Get-ItemProperty -LiteralPath $_ -Name 'pv' -ErrorAction SilentlyContinue).pv
            if ($version -and $version -ne '0.0.0.0') { $version }
        }
    } | Select-Object -First 1
    if ($runtimeVersion) { New-Record 'webview2.json' ([ordered]@{ status = 'already-installed'; version = $runtimeVersion }) }
    elseif (-not (Test-Path -LiteralPath $installer)) { throw 'WebView2 runtime is absent and no staged installer exists.' }
    else {
        $signature = Get-AuthenticodeSignature -LiteralPath $installer
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft') { throw 'WebView2 Authenticode signature is not a valid Microsoft signature.' }
        $p = Start-Process -FilePath $installer -ArgumentList @('/silent','/install') -Wait -PassThru -NoNewWindow
        if ($p.ExitCode -notin @(0, 1638)) { throw "WebView2 installer exit $($p.ExitCode)" }
        New-Record 'webview2.json' ([ordered]@{ status = 'installed'; exit_code = $p.ExitCode })
    }
}
$webviewGate = $steps[-1]
if ($webviewGate.status -ne 'passed') { Complete-Run $steps 'failed' $manifestHash; exit 1 }
$steps += Invoke-Step 'msi-quiet-install' {
    $log = Join-Path $evidence 'msi-install.log'
    $p = Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i', (Quote-ProcessArgument (Join-Path $inputRoot 'Agent-Manager.msi')), '/qn', '/norestart', '/L*v', (Quote-ProcessArgument $log)) -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "MSI install exit $($p.ExitCode)" }
}
$steps += Invoke-Step 'msi-launch-render-evidence' {
    $app = Get-ShortcutTarget
    if (-not $app) { throw 'Installed Agent Manager executable was not found.' }
    $p = Start-Process -FilePath $app -PassThru
    try {
        Start-Sleep -Seconds 5
        $p.Refresh()
        if ($p.HasExited) { throw "Installed application exited with code $($p.ExitCode)." }
        if ($p.MainWindowHandle -eq 0 -or [string]::IsNullOrWhiteSpace($p.MainWindowTitle)) { throw 'Installed Agent Manager window is not visible.' }
        Save-Screenshot (Join-Path $evidence 'msi-render.png')
        New-Record 'msi-render.json' ([ordered]@{ process_id = $p.Id; window_title = $p.MainWindowTitle; launched = $true; rendered = $true; screenshot = 'msi-render.png' })
    } finally {
        $p.Refresh()
        if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    }
}
$steps += Invoke-Step 'msi-uninstall' {
    $log = Join-Path $evidence 'msi-uninstall.log'
    $p = Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/x', (Quote-ProcessArgument (Join-Path $inputRoot 'Agent-Manager.msi')), '/qn', '/norestart', '/L*v', (Quote-ProcessArgument $log)) -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "MSI uninstall exit $($p.ExitCode)" }
}
$steps += Invoke-Step 'portable-launch-render-evidence' {
    $portableRoot = Join-Path $env:TEMP "AgentManagerSmoke-$RunId"
    Expand-Archive -LiteralPath (Join-Path $inputRoot 'Agent-Manager-Portable.zip') -DestinationPath $portableRoot -Force
    $app = Get-ChildItem -LiteralPath $portableRoot -Filter 'agent-manager.exe' -Recurse -File
    if (@($app).Count -ne 1) { throw 'Portable agent-manager.exe contract was not satisfied.' }
    $app = $app[0]
    if (-not $app) { throw 'Portable application executable was not found.' }
    $p = Start-Process -FilePath $app.FullName -PassThru
    try {
        Start-Sleep -Seconds 5
        $p.Refresh()
        if ($p.HasExited) { throw "Portable application exited with code $($p.ExitCode)." }
        if ($p.MainWindowHandle -eq 0 -or [string]::IsNullOrWhiteSpace($p.MainWindowTitle)) { throw 'Portable Agent Manager window is not visible.' }
        Save-Screenshot (Join-Path $evidence 'portable-render.png')
        New-Record 'portable-render.json' ([ordered]@{ process_id = $p.Id; window_title = $p.MainWindowTitle; launched = $true; rendered = $true; screenshot = 'portable-render.png' })
    } finally {
        $p.Refresh()
        if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    }
}
$failed = @($steps | Where-Object { $_.status -ne 'passed' })
$status = if ($failed.Count -eq 0) { 'passed' } else { 'failed' }
Complete-Run $steps $status $manifestHash
if ($failed.Count -ne 0) { exit 1 }
