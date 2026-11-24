#Requires -Version 7.6

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $RepositoryRoot,
    [Parameter(Mandatory)][ValidatePattern('(?-i)\A[0-9a-f]{40}\z')][string] $Commit,
    [Parameter(Mandatory)][string] $OutputDirectory
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

# Release callers establish tag trust separately. This helper only archives
# the selected commit in a trusted checkout without local attribute overrides.
$repository = Get-Item -LiteralPath $RepositoryRoot
$output = Get-Item -LiteralPath $OutputDirectory
if (-not $repository.PSIsContainer -or -not $output.PSIsContainer) {
    throw 'RepositoryRoot and OutputDirectory must be existing directories.'
}
$kind = & git --no-replace-objects --no-lazy-fetch -C $repository.FullName cat-file -t $Commit
if ($LASTEXITCODE -ne 0 -or $kind -cne 'commit') {
    throw 'Commit must identify a local Git commit object.'
}

$name = "azd-gui-$Commit"
$archive = Join-Path $output.FullName "$name.tar.gz"
if (Test-Path -LiteralPath $archive) {
    throw 'The requested source bundle already exists.'
}
$temporary = Join-Path $output.FullName ".$([guid]::NewGuid().ToString('N')).tmp"
try {
    $gitArgs = @(
        '--no-replace-objects', '--no-lazy-fetch', '-C', $repository.FullName
        '-c', 'tar.tar.gz.command=git archive gzip', '-c', 'tar.umask=0002'
        'archive', '--format=tar.gz', "--prefix=$name/", "--output=$temporary", $Commit
    )
    & git @gitArgs
    if ($LASTEXITCODE -ne 0) { throw 'Git could not create the source bundle.' }

    $hash = (Get-FileHash -LiteralPath $temporary -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::Move($temporary, $archive)
    [pscustomobject]@{ Path = $archive; Commit = $Commit; Sha256 = $hash }
}
finally {
    if (Test-Path -LiteralPath $temporary) {
        Remove-Item -LiteralPath $temporary
    }
}
