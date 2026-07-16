Describe 'Manifest canonicalization contract' {
    It 'is identical in PowerShell 7 and Windows PowerShell 5.1' {
        $root = Split-Path $PSScriptRoot -Parent
        $contract = Join-Path $root 'ManifestContract.ps1'
        . $contract

        $manifestPath = Join-Path $TestDrive 'manifest.json'
        $manifest = [ordered]@{
            schema = 1
            run_id = 'cross-version'
            scenario = 'package-smoke'
            network_mode = 'offline'
            files = @(
                [ordered]@{ path = 'Agent-Manager-Portable.zip'; length = 1; sha256 = ('a' * 64) },
                [ordered]@{ path = 'Agent-Manager.msi'; length = 2; sha256 = ('b' * 64) },
                [ordered]@{ path = 'ManifestContract.ps1'; length = 3; sha256 = ('c' * 64) },
                [ordered]@{ path = 'Run-WindowsSandboxSmoke.ps1'; length = 4; sha256 = ('d' * 64) },
                [ordered]@{ path = 'SHA256SUMS.txt'; length = 5; sha256 = ('e' * 64) }
            )
        }
        [IO.File]::WriteAllText($manifestPath, ($manifest | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))

        $currentPayload = Get-ManifestPayloadJson $manifest
        $command = ". '$($contract.Replace("'", "''"))'; `$manifest = Get-Content -LiteralPath '$($manifestPath.Replace("'", "''"))' -Raw | ConvertFrom-Json; Get-ManifestPayloadJson `$manifest"
        $windowsPayload = (& powershell.exe -NoProfile -ExecutionPolicy Bypass -Command $command | Out-String).Trim()

        $windowsPayload | Should Be $currentPayload
        @((ConvertFrom-Json $currentPayload).files | ForEach-Object { $_.path }) | Should Be @(
            'Agent-Manager-Portable.zip',
            'Agent-Manager.msi',
            'ManifestContract.ps1',
            'Run-WindowsSandboxSmoke.ps1',
            'SHA256SUMS.txt'
        )
    }
}
