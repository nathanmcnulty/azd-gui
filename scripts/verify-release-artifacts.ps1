[CmdletBinding()]
param(
    [Parameter(Mandatory)]
    [string] $ReleaseDirectory,

    [Parameter(Mandatory)]
    [ValidatePattern('^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$')]
    [string] $ExpectedVersion,

    [Parameter(Mandatory)]
    [string] $ExpectedSignerSubject,

    [string] $EvidencePath
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$releaseRoot = (Resolve-Path -LiteralPath $ReleaseDirectory).Path
$checksumPath = Join-Path $releaseRoot 'SHA256SUMS.txt'
if (-not (Test-Path -LiteralPath $checksumPath -PathType Leaf)) {
    throw "Missing release checksum file: $checksumPath"
}

$entries = [System.Collections.Generic.List[object]]::new()
$seenNames = [System.Collections.Generic.HashSet[string]]::new(
    [System.StringComparer]::OrdinalIgnoreCase
)
foreach ($line in Get-Content -LiteralPath $checksumPath) {
    if ([string]::IsNullOrWhiteSpace($line)) {
        continue
    }

    if ($line -notmatch '^(?<hash>[0-9a-fA-F]{64})  (?<name>[^\\/]+\.exe)$') {
        throw "Invalid checksum entry: $line"
    }
    if (-not $seenNames.Add($Matches.name)) {
        throw "Duplicate checksum entry for '$($Matches.name)'."
    }

    $entries.Add([pscustomobject]@{
        ExpectedHash = $Matches.hash.ToLowerInvariant()
        Name = $Matches.name
    })
}

if ($entries.Count -ne 2 -or -not $seenNames.Contains('azd-gui.exe')) {
    throw 'Expected checksums for azd-gui.exe and exactly one NSIS installer.'
}
$installerName = @($entries.Name | Where-Object { $_ -ne 'azd-gui.exe' })
if ($installerName.Count -ne 1 -or $installerName[0] -notlike '*_x64-setup.exe') {
    throw 'Expected one x64 NSIS installer with a name ending in _x64-setup.exe.'
}
$unlistedExecutables = @(
    Get-ChildItem -LiteralPath $releaseRoot -Filter '*.exe' -File |
        Where-Object { -not $seenNames.Contains($_.Name) }
)
if ($unlistedExecutables.Count -gt 0) {
    throw "Release directory contains an executable absent from SHA256SUMS.txt: $($unlistedExecutables[0].Name)"
}

$results = foreach ($entry in $entries) {
    $path = Join-Path $releaseRoot $entry.Name
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "Missing release artifact: $path"
    }

    $actualHash = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -cne $entry.ExpectedHash) {
        throw "SHA-256 mismatch for '$($entry.Name)'."
    }

    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
        throw "Invalid Authenticode signature on '$($entry.Name)': $($signature.Status)"
    }
    if ($signature.SignerCertificate.Subject -cne $ExpectedSignerSubject) {
        throw "Unexpected signer on '$($entry.Name)': $($signature.SignerCertificate.Subject)"
    }
    if (-not $signature.TimeStamperCertificate) {
        throw "Missing trusted timestamp on '$($entry.Name)'."
    }

    [pscustomobject]@{
        Name = $entry.Name
        Sha256 = $actualHash
        SignatureStatus = $signature.Status.ToString()
        SignerSubject = $signature.SignerCertificate.Subject
        SignerThumbprint = $signature.SignerCertificate.Thumbprint
        TimestampSubject = $signature.TimeStamperCertificate.Subject
    }
}

$portablePath = Join-Path $releaseRoot 'azd-gui.exe'
$productVersion = (Get-Item -LiteralPath $portablePath).VersionInfo.ProductVersion
if (-not $productVersion -or $productVersion -cne $ExpectedVersion) {
    throw "Portable executable version '$productVersion' does not match '$ExpectedVersion'."
}

if (-not $EvidencePath) {
    $EvidencePath = Join-Path $releaseRoot 'artifact-verification.json'
}
$evidence = [ordered]@{
    verifiedAtUtc = [DateTime]::UtcNow.ToString('o')
    windowsVersion = [System.Environment]::OSVersion.VersionString
    expectedVersion = $ExpectedVersion
    expectedSignerSubject = $ExpectedSignerSubject
    artifacts = @($results)
}
$evidence | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $EvidencePath -Encoding utf8

$results | Format-Table Name, SignatureStatus, SignerSubject -AutoSize
Write-Host "Artifact evidence written to $EvidencePath"
