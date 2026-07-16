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

function Get-ManifestFilesOrdinal($Manifest) {
    $filesByPath = @{}
    foreach ($file in @($Manifest.files)) {
        $path = [string]$file.path
        if ($filesByPath.ContainsKey($path)) { throw "Duplicate manifest path: $path" }
        $filesByPath.Add($path, $file)
    }

    [string[]]$paths = @($filesByPath.Keys)
    [Array]::Sort($paths, [StringComparer]::Ordinal)
    return @($paths | ForEach-Object {
            $file = $filesByPath[$_]
            [ordered]@{ path = [string]$file.path; length = [int64]$file.length; sha256 = [string]$file.sha256 }
        })
}

function Get-ManifestPayloadJson($Manifest) {
    $payload = [ordered]@{
        schema = [int]$Manifest.schema
        run_id = [string]$Manifest.run_id
        scenario = [string]$Manifest.scenario
        network_mode = [string]$Manifest.network_mode
        files = @(Get-ManifestFilesOrdinal $Manifest)
    }
    return ($payload | ConvertTo-Json -Depth 8 -Compress)
}
