Describe 'Windows Sandbox smoke harness static contract' {
    $root = Split-Path $PSScriptRoot -Parent
    It 'contains separate host launcher and Sandbox runner' {
        (Test-Path (Join-Path $root 'Invoke-WindowsSandboxSmoke.ps1')) | Should Be $true
        (Test-Path (Join-Path $root 'Run-WindowsSandboxSmoke.ps1')) | Should Be $true
    }
    It 'requires explicit launch and uses only two non-overlapping mappings' {
        $text = Get-Content (Join-Path $root 'Invoke-WindowsSandboxSmoke.ps1') -Raw
        $text | Should Match '<ReadOnly>true</ReadOnly>'
        $text | Should Match '<ReadOnly>false</ReadOnly>'
        $text | Should Match '<Networking>Disable</Networking>'
        $text | Should Match '<ClipboardRedirection>Disable</ClipboardRedirection>'
        $text | Should Match '<PrinterRedirection>Disable</PrinterRedirection>'
        $text | Should Match '<AudioInput>Disable</AudioInput>'
        $text | Should Match '<VideoInput>Disable</VideoInput>'
        $text | Should Match '<VGpu>Disable</VGpu>'
        $text | Should Match '\$Launch'
        $text | Should Not Match 'test\\</HostFolder>'
    }
    It 'finalizes evidence with complete.json after required smoke steps' {
        $text = Get-Content (Join-Path $root 'Run-WindowsSandboxSmoke.ps1') -Raw
        $text.IndexOf("New-Record 'complete.json'") | Should BeGreaterThan $text.IndexOf("New-Record 'steps.json'")
        $text | Should Match 'artifact-and-hash-contract'
        $text | Should Match 'msi-quiet-install'
        $text | Should Match 'msi-uninstall'
        $text | Should Match 'portable-launch-render-evidence'
        $text | Should Match 'manifest_sha256 = \$ManifestHash'
        $text | Should Match 'Get-ManifestPayloadJson'
        $text | Should Match 'recomputedManifestHash'
        $text | Should Match 'actualFile.Length'
        $text | Should Match 'IsPathRooted'
        $text | Should Match 'Duplicate manifest path'
        $text | Should Match 'artifactGate'
        $text | Should Match 'webviewGate'
        $text.IndexOf("Complete-Run `$steps 'failed' `$manifestHash; exit 1") | Should BeLessThan $text.IndexOf("'msi-quiet-install'")
    }
    It 'uses the approved Evergreen WebView2 client GUID and registry views' {
        $text = Get-Content (Join-Path $root 'Run-WindowsSandboxSmoke.ps1') -Raw
        $text | Should Match '\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5\}'
        $text | Should Match 'WOW6432Node'
        $text | Should Match 'HKCU:'
        $text | Should Match "-Name 'pv'"
        $text | Should Match "0\.0\.0\.0"
        $text | Should Not Match 'F1E7E8E1'
    }
    It 'recomputes the manifest payload hash independently before the artifact gate' {
        $text = Get-Content (Join-Path $root 'Run-WindowsSandboxSmoke.ps1') -Raw
        $text.IndexOf('recomputedManifestHash') | Should BeLessThan $text.IndexOf("'artifact-and-hash-contract'")
        $text | Should Match 'UTF8.GetBytes\(\(Get-ManifestPayloadJson \$manifest\)\)'
        $text | Should Match 'manifest payload hash mismatch'
    }
}
