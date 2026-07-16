Describe 'Windows Sandbox smoke prepare-only contract' {
    It 'creates fixed staged names and a safe two-mapping configuration' {
        $root = Split-Path $PSScriptRoot -Parent
        $source = Join-Path $TestDrive 'source'
        $evidence = Join-Path $TestDrive 'evidence'
        New-Item -ItemType Directory -Path $source,$evidence | Out-Null
        $msi = Join-Path $source 'Agent-Manager-0.1.0-Windows.msi'
        $portable = Join-Path $source 'Agent-Manager-0.1.0-Windows-Portable.zip'
        $webview = Join-Path $source 'MicrosoftEdgeWebView2RuntimeInstallerX64.exe'
        [IO.File]::WriteAllBytes($msi, [byte[]](1,2,3))
        [IO.File]::WriteAllBytes($portable, [byte[]](4,5,6))
        [IO.File]::WriteAllBytes($webview, [byte[]](7,8,9))
        $msiHash = (Get-FileHash $msi -Algorithm SHA256).Hash.ToLowerInvariant()
        $portableHash = (Get-FileHash $portable -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath (Join-Path $source 'SHA256SUMS.txt') -Value "$msiHash  $($msi | Split-Path -Leaf)`n$portableHash  $($portable | Split-Path -Leaf)" -Encoding ASCII

        & (Join-Path $root 'Invoke-WindowsSandboxSmoke.ps1') -MsiPath $msi -PortableZipPath $portable -ChecksumsPath (Join-Path $source 'SHA256SUMS.txt') -WebView2Path $webview -EvidenceRoot $evidence -PrepareOnly | Out-Null
        $run = Get-ChildItem -LiteralPath $evidence -Directory
        @($run).Count | Should Be 1
        $control = Join-Path $run.FullName 'control'
        $input = Join-Path $run.FullName 'input'
        $output = Join-Path $run.FullName 'sandbox-output'
        foreach ($name in @('Agent-Manager.msi','Agent-Manager-Portable.zip','SHA256SUMS.txt','MicrosoftEdgeWebView2RuntimeInstallerX64.exe','manifest.json','Run-WindowsSandboxSmoke.ps1','ManifestContract.ps1')) { Test-Path (Join-Path $input $name) | Should Be $true }
        $config = [xml](Get-Content (Join-Path $control 'package-smoke.wsb') -Raw)
        $maps = @($config.Configuration.MappedFolders.MappedFolder)
        $maps.Count | Should Be 2
        $maps[0].ReadOnly | Should Be 'true'
        $maps[1].ReadOnly | Should Be 'false'
        $maps[0].HostFolder | Should Be $input
        $maps[1].HostFolder | Should Be $output
        ($config.Configuration.LogonCommand.Command -match 'WindowsSandbox\.exe|msiexec\.exe|agent-manager\.exe') | Should Be $false
        (Test-Path (Join-Path $control 'launch.json')) | Should Be $true
        (Test-Path (Join-Path $run.FullName 'complete.json')) | Should Be $false
        $config.Configuration.Networking | Should Be 'Disable'
        $config.Configuration.ClipboardRedirection | Should Be 'Disable'
        $config.Configuration.PrinterRedirection | Should Be 'Disable'
        $config.Configuration.AudioInput | Should Be 'Disable'
        $config.Configuration.VideoInput | Should Be 'Disable'
        $config.Configuration.VGpu | Should Be 'Disable'
        $config.Configuration.MemoryInMB | Should Be '2048'
        $controlManifest = Get-Content (Join-Path $control 'manifest.json') -Raw | ConvertFrom-Json
        $payload = [ordered]@{
            schema = [int]$controlManifest.schema
            run_id = [string]$controlManifest.run_id
            scenario = [string]$controlManifest.scenario
            network_mode = [string]$controlManifest.network_mode
            files = @($controlManifest.files | Sort-Object { [string]$_.path } | ForEach-Object {
                    [ordered]@{ path = [string]$_.path; length = [int64]$_.length; sha256 = [string]$_.sha256 }
                })
        }
        $payloadJson = $payload | ConvertTo-Json -Depth 8 -Compress
        $computed = [BitConverter]::ToString(([Security.Cryptography.SHA256]::Create().ComputeHash([Text.Encoding]::UTF8.GetBytes($payloadJson)))).Replace('-','').ToLowerInvariant()
        $computed | Should Be $controlManifest.manifest_sha256
    }
}
