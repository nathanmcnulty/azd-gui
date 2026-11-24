#Requires -Version 7.6

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $ReleaseDirectory,
    [Parameter(Mandatory)][ValidatePattern('(?-i)\A[0-9a-f]{40}\z')][string] $Commit,
    [Parameter(Mandatory)][ValidatePattern('(?-i)\A[0-9a-f]{64}\z')][string] $EvidenceSha256,
    [Parameter(Mandatory)][ValidatePattern('(?-i)\A[0-9a-f]{64}\z')][string] $SourceSha256
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Assert-ReleaseHash($Path, $Sha256) {
    $file = Get-Item -LiteralPath $Path
    if ($file.PSIsContainer -or $file.LinkType -or
        (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash -ine $Sha256) {
        throw "Release handoff hash or file type mismatch for '$($file.Name)'."
    }
}

# This receipt is emitted by the Windows signature and installed-payload
# verification. Its digest is passed separately from the immutable artifact.
$evidence = Join-Path $ReleaseDirectory 'installed-artifact-verification.json'
Assert-ReleaseHash $evidence $EvidenceSha256
$receipt = Get-Content -LiteralPath $evidence -Raw | ConvertFrom-Json
$portable = @($receipt.artifacts | Where-Object { $_.Name -ceq 'azd-gui.exe' })
$installer = @($receipt.artifacts | Where-Object {
    $_.Name -cmatch '\A[A-Za-z0-9][A-Za-z0-9 ._+-]*_x64-setup\.exe\z'
})
if ($receipt.artifacts.Count -ne 2 -or $portable.Count -ne 1 -or $installer.Count -ne 1 -or
    $receipt.installedExecutable.signatureStatus -cne 'Valid' -or
    @($receipt.artifacts | Where-Object {
        $_.SignatureStatus -cne 'Valid' -or $_.Sha256 -cnotmatch '\A[0-9a-f]{64}\z'
    }).Count -ne 0) {
    throw 'The release handoff requires the verified portable, installer, and installed payload.'
}

$files = [pscustomobject]@{
    Portable = Join-Path $ReleaseDirectory $portable[0].Name
    Installer = Join-Path $ReleaseDirectory 'bundle/nsis' $installer[0].Name
    Source = Join-Path $ReleaseDirectory "azd-gui-$Commit.tar.gz"
    Checksums = Join-Path $ReleaseDirectory 'SHA256SUMS.txt'
}
Assert-ReleaseHash $files.Portable $portable[0].Sha256
Assert-ReleaseHash $files.Installer $installer[0].Sha256
Assert-ReleaseHash $files.Source $SourceSha256

$checksums = @($receipt.artifacts | ForEach-Object { "$($_.Sha256)  $($_.Name)" })
$actual = @(Get-Content -LiteralPath $files.Checksums)
if ($actual.Count -ne $checksums.Count -or (Compare-Object $actual $checksums -CaseSensitive)) {
    throw 'SHA256SUMS.txt differs from the verified Windows artifacts.'
}
$files
