[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$RunId)
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$inputRoot = 'C:\AgentManagerHarness\Input'
$evidence = 'C:\AgentManagerHarness\Evidence'
. (Join-Path $PSScriptRoot 'ManifestContract.ps1')

function New-Record([string]$Name, $Value) {
    $path = Join-Path $evidence $Name
    $file = New-Object IO.FileStream($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    $stream = New-Object IO.StreamWriter($file, (New-Object Text.UTF8Encoding($false)))
    try { $stream.Write(($Value | ConvertTo-Json -Depth 8)) } finally { $stream.Dispose(); $file.Dispose() }
}
function Quote-ProcessArgument([string]$Value) { return '"' + $Value.Replace('"', '\"') + '"' }
function Wait-ProcessBounded($Process, [int]$TimeoutSeconds, [string]$Name) {
    if (-not $Process.WaitForExit($TimeoutSeconds * 1000)) {
        try { Stop-Process -Id $Process.Id -Force } catch {}
        throw "$Name timed out after $TimeoutSeconds seconds."
    }
}
function Get-InstalledApp {
    $candidate = Join-Path $env:LOCALAPPDATA 'Programs\Agent Manager\agent-manager.exe'
    if (Test-Path -LiteralPath $candidate -PathType Leaf) { return $candidate }
    $shell = New-Object -ComObject WScript.Shell
    foreach ($shortcut in Get-ChildItem (Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs') -Filter '*Agent Manager*.lnk' -Recurse -File -ErrorAction SilentlyContinue) {
        $target = $shell.CreateShortcut($shortcut.FullName).TargetPath
        if ($target -and (Test-Path -LiteralPath $target -PathType Leaf)) { return $target }
    }
    return $null
}

try {
    $manifest = Get-Content -LiteralPath (Join-Path $inputRoot 'manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.run_id -ne $RunId -or $manifest.scenario -ne 'guided-install' -or $manifest.network_mode -ne 'online') { throw 'Guided manifest identity mismatch.' }
    $hash = [string]$manifest.manifest_sha256
    $recomputed = [BitConverter]::ToString(([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes((Get-ManifestPayloadJson $manifest))))).Replace('-','').ToLowerInvariant()
    if ($hash -ne $recomputed) { throw 'Guided manifest payload hash mismatch.' }
    foreach ($entry in @($manifest.files)) {
        $path = [string]$entry.path
        if ([IO.Path]::IsPathRooted($path) -or $path.Contains('\') -or $path.Contains('/')) { throw "Unsafe guided manifest path: $path" }
        $actual = Join-Path $inputRoot $path
        if (-not (Test-Path -LiteralPath $actual -PathType Leaf)) { throw "Missing guided input: $path" }
        if ((Get-Item -LiteralPath $actual).Length -ne (Convert-ManifestLength $entry.length)) { throw "Guided input length mismatch: $path" }
        if ((Get-FileHash -LiteralPath $actual -Algorithm SHA256).Hash.ToLowerInvariant() -ne $entry.sha256) { throw "Guided input hash mismatch: $path" }
    }

    New-Record 'startup.json' ([ordered]@{ schema = 1; run_id = $RunId; status = 'preparing'; started_utc = [DateTime]::UtcNow.ToString('o') })
    $webview = Join-Path $inputRoot 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
    $signature = Get-AuthenticodeSignature -LiteralPath $webview
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'Microsoft') { throw 'WebView2 signature is invalid.' }
    $webviewProcess = Start-Process -FilePath $webview -ArgumentList @('/silent','/install') -PassThru -NoNewWindow
    Wait-ProcessBounded $webviewProcess 180 'WebView2 installation'
    if ($webviewProcess.ExitCode -notin @(0, 1638)) { throw "WebView2 installer exit $($webviewProcess.ExitCode)" }

    $msiLog = Join-Path $evidence 'agent-manager-install.log'
    $msi = Start-Process -FilePath 'msiexec.exe' -ArgumentList @('/i', (Quote-ProcessArgument (Join-Path $inputRoot 'Agent-Manager.msi')), '/qn', '/norestart', '/L*v', (Quote-ProcessArgument $msiLog)) -PassThru
    Wait-ProcessBounded $msi 120 'Agent Manager installation'
    if ($msi.ExitCode -ne 0) { throw "Agent Manager MSI exit $($msi.ExitCode)" }

    $appPath = Get-InstalledApp
    if (-not $appPath) { throw 'Installed Agent Manager executable was not found.' }
    $app = Start-Process -FilePath $appPath -PassThru
    Start-Sleep -Seconds 7
    $app.Refresh()
    if ($app.HasExited) { throw "Agent Manager exited with code $($app.ExitCode)." }
    if ($app.MainWindowTitle -cne 'Agent Manager') { throw "Unexpected Agent Manager window title: $($app.MainWindowTitle)" }
    New-Record 'ready.json' ([ordered]@{ schema = 1; run_id = $RunId; status = 'ready'; app_path = $appPath; process_id = $app.Id; window_title = $app.MainWindowTitle; ready_utc = [DateTime]::UtcNow.ToString('o') })
} catch {
    New-Record 'startup-failed.json' ([ordered]@{ schema = 1; run_id = $RunId; status = 'failed'; error = $_.Exception.Message; failed_utc = [DateTime]::UtcNow.ToString('o') })
    exit 1
}

