Describe 'Windows Sandbox guided install contract' {
    It 'prepares an isolated online two-mapping run with bounded memory' {
        $root = Split-Path $PSScriptRoot -Parent
        $source = Join-Path $TestDrive 'source'
        $runtime = Join-Path $TestDrive 'runtime'
        New-Item -ItemType Directory -Path $source,$runtime | Out-Null
        $msi = Join-Path $source 'candidate.msi'
        $webview = Join-Path $source 'webview.exe'
        [IO.File]::WriteAllBytes($msi, [byte[]](1,2,3))
        [IO.File]::WriteAllBytes($webview, [byte[]](4,5,6))
        $hash = (Get-FileHash $msi -Algorithm SHA256).Hash.ToLowerInvariant()
        $sums = Join-Path $source 'SHA256SUMS.txt'
        Set-Content -LiteralPath $sums -Value "$hash  candidate.msi" -Encoding ASCII

        Mock Get-PSDrive { [pscustomobject]@{ Free = 30GB } } -ParameterFilter { $Name -eq 'C' }
        & (Join-Path $root 'Invoke-WindowsSandboxGuidedInstall.ps1') -MsiPath $msi -ChecksumsPath $sums -WebView2Path $webview -RuntimeRoot $runtime -PrepareOnly | Out-Null
        $run = Get-ChildItem (Join-Path $runtime 'runs') -Directory
        @($run).Count | Should Be 1
        $config = [xml](Get-Content (Join-Path $run.FullName 'control\guided-install.wsb') -Raw)
        $config.Configuration.Networking | Should Be 'Default'
        $config.Configuration.MemoryInMB | Should Be '2048'
        $config.Configuration.ClipboardRedirection | Should Be 'Disable'
        $maps = @($config.Configuration.MappedFolders.MappedFolder)
        $maps.Count | Should Be 2
        $maps[0].ReadOnly | Should Be 'true'
        $maps[1].ReadOnly | Should Be 'false'
        (Test-Path (Join-Path $run.FullName 'input\Run-WindowsSandboxGuidedInstall.ps1')) | Should Be $true
        (Test-Path (Join-Path $run.FullName 'input\Collect-WindowsSandboxGuidedInstall.ps1')) | Should Be $true
        $manifest = Get-Content (Join-Path $run.FullName 'control\manifest.json') -Raw | ConvertFrom-Json
        $manifest.scenario | Should Be 'guided-install'
        $manifest.network_mode | Should Be 'online'
    }

    It 'refuses preparation below the C drive free-space floor' {
        $root = Split-Path $PSScriptRoot -Parent
        Mock Get-PSDrive { [pscustomobject]@{ Free = 19GB } } -ParameterFilter { $Name -eq 'C' }
        { & (Join-Path $root 'Invoke-WindowsSandboxGuidedInstall.ps1') -MsiPath 'missing' -ChecksumsPath 'missing' -WebView2Path 'missing' -RuntimeRoot $TestDrive -PrepareOnly } | Should Throw 'Windows Sandbox launch refused because C: has less than 20 GiB free.'
    }

    It 'collects node npm and codex post-install versions' {
        $root = Split-Path $PSScriptRoot -Parent
        $text = Get-Content (Join-Path $root 'Collect-WindowsSandboxGuidedInstall.ps1') -Raw
        $text | Should Match "Test-CommandVersion 'node'"
        $text | Should Match "Test-CommandVersion 'npm'"
        $text | Should Match 'Test-CommandVersion \$Tool'
        $text | Should Match "GetEnvironmentVariable\('Path', 'User'\)"
        $text | Should Match "'guided-result.json'"
    }

    It 'refuses overlapping Sandbox execution and reparse runtime roots' {
        $root = Split-Path $PSScriptRoot -Parent
        $text = Get-Content (Join-Path $root 'Invoke-WindowsSandboxGuidedInstall.ps1') -Raw
        $text | Should Match 'Another Windows Sandbox scenario is already running'
        $text | Should Match 'runtime root cannot be a reparse point'
    }
}
