Describe 'Manifest length contract' {
    . (Join-Path (Split-Path $PSScriptRoot -Parent) 'ManifestContract.ps1')

    It 'rejects fractional lengths without truncation' {
        Convert-ManifestLength ([double]1.5) | Should Be $null
        Convert-ManifestLength ([decimal]2.25) | Should Be $null
    }

    It 'accepts exact nonnegative Int64 values only' {
        Convert-ManifestLength ([int64]0) | Should Be 0
        Convert-ManifestLength ([double]3.0) | Should Be 3
        Convert-ManifestLength ([string]'4') | Should Be $null
        Convert-ManifestLength ([bool]$false) | Should Be $null
        Convert-ManifestLength ([double]::NaN) | Should Be $null
        Convert-ManifestLength ([double]::PositiveInfinity) | Should Be $null
        Convert-ManifestLength ([int64]-1) | Should Be $null
    }
}
