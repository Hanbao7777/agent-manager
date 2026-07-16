[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$RunId,
    [ValidateSet('codex')][string]$Tool = 'codex'
)
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'
$evidence = 'C:\AgentManagerHarness\Evidence'

function Test-CommandVersion([string]$Command) {
    $resolved = Get-Command $Command -ErrorAction SilentlyContinue | Select-Object -First 1
    if (-not $resolved) { return [ordered]@{ command = $Command; status = 'missing' } }
    $output = @(& $resolved.Source '--version' 2>&1)
    $exitCode = $LASTEXITCODE
    return [ordered]@{ command = $Command; status = if ($exitCode -eq 0) { 'passed' } else { 'failed' }; path = $resolved.Source; exit_code = $exitCode; version = ($output -join "`n") }
}

$checks = @(
    (Test-CommandVersion 'node'),
    (Test-CommandVersion 'npm'),
    (Test-CommandVersion $Tool)
)
$status = if (@($checks | Where-Object { $_.status -ne 'passed' }).Count -eq 0) { 'passed' } else { 'failed' }
$record = [ordered]@{
    schema = 1
    run_id = $RunId
    scenario = 'guided-install'
    tool = $Tool
    status = $status
    collected_utc = [DateTime]::UtcNow.ToString('o')
    process_path = $env:PATH
    user_path = [Environment]::GetEnvironmentVariable('Path', 'User')
    checks = $checks
}
$path = Join-Path $evidence 'guided-result.json'
$file = New-Object IO.FileStream($path, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
$stream = New-Object IO.StreamWriter($file, (New-Object Text.UTF8Encoding($false)))
try { $stream.Write(($record | ConvertTo-Json -Depth 8)) } finally { $stream.Dispose(); $file.Dispose() }
if ($status -ne 'passed') { exit 1 }

