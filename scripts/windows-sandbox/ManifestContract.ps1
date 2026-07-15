function Convert-ManifestLength([object]$Value) {
    if ($null -eq $Value -or $Value -is [bool] -or $Value -is [string]) { return $null }
    if ($Value -is [double]) {
        if ([double]::IsNaN($Value) -or [double]::IsInfinity($Value) -or [Math]::Truncate($Value) -ne $Value) { return $null }
    }
    if ($Value -is [decimal] -and [Math]::Truncate($Value) -ne $Value) { return $null }
    try {
        $result = [Convert]::ToInt64($Value)
        if ($result -lt 0) { return $null }
        if ($Value -is [double] -and [double]$result -ne $Value) { return $null }
        if ($Value -is [decimal] -and [decimal]$result -ne $Value) { return $null }
        return $result
    } catch { return $null }
}
