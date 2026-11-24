#Requires -Version 7.6

[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidatePattern('(?-i)\Arefs/tags/v[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.+-]+)?\z')][string] $ReleaseRef,
    [Parameter(Mandatory)][ValidatePattern('(?-i)\A[0-9a-f]{40}\z')][string] $EventCommit,
    [ValidatePattern('(?-i)\A[0-9a-f]{40}\z')][string] $ExpectedTagObject
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false

# The v* ruleset protects the public ref. Compare its exact annotated object
# to the checkout, GitHub's signature verification and the pinned signer.
$repository = 'nathanmcnulty/azd-gui'
$tagName = $ReleaseRef.Substring('refs/tags/'.Length)
$encodedTag = [Uri]::EscapeDataString($tagName)
$refJson = & gh api --hostname github.com "repos/$repository/git/ref/tags/$encodedTag"
if ($LASTEXITCODE -ne 0) { throw 'Could not read the exact release tag ref.' }
$remoteRef = $refJson | ConvertFrom-Json
if ($remoteRef.ref -cne $ReleaseRef -or $remoteRef.object.type -cne 'tag' -or
    $remoteRef.object.sha -cnotmatch '\A[0-9a-f]{40}\z') {
    throw 'The release ref must identify an exact annotated tag.'
}

$tagObject = $remoteRef.object.sha
if ($PSBoundParameters.ContainsKey('ExpectedTagObject') -and $tagObject -cne $ExpectedTagObject) {
    throw 'The remote tag object changed after Windows verification.'
}
$tagJson = & gh api --hostname github.com "repos/$repository/git/tags/$tagObject"
if ($LASTEXITCODE -ne 0) { throw 'Could not read the exact annotated tag.' }
$remoteTag = $tagJson | ConvertFrom-Json
if ($remoteTag.sha -cne $tagObject -or $remoteTag.tag -cne $tagName -or
    $remoteTag.object.type -cne 'commit' -or $remoteTag.object.sha -cne $EventCommit -or
    $remoteTag.verification.verified -isnot [bool] -or -not $remoteTag.verification.verified) {
    throw 'GitHub did not verify the annotated tag for the exact event commit.'
}

$localTag = & git --no-replace-objects rev-parse --verify "${ReleaseRef}^{tag}"
if ($LASTEXITCODE -ne 0 -or $localTag -cne $tagObject) {
    throw 'The checked-out tag object differs from the protected remote tag.'
}
$head = & git --no-replace-objects rev-parse --verify 'HEAD^{commit}'
if ($LASTEXITCODE -ne 0 -or $head -cne $EventCommit) {
    throw 'HEAD differs from the verified release commit.'
}
& git --no-replace-objects merge-base --is-ancestor $EventCommit refs/remotes/origin/main
if ($LASTEXITCODE -ne 0) { throw 'The verified release commit must be on main.' }

$signers = Join-Path $PSScriptRoot 'release-tag-allowed-signers'
$null = & git --no-replace-objects -c gpg.format=ssh -c gpg.minTrustLevel=fully -c "gpg.ssh.allowedSignersFile=$signers" verify-tag $tagObject
if ($LASTEXITCODE -ne 0) { throw 'The release tag does not match the pinned signer.' }
$tagObject
