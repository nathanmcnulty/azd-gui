[CmdletBinding()]
param([Parameter(Mandatory)][string] $Path)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

foreach ($name in @('ARTIFACT_SIGNING_ENDPOINT', 'ARTIFACT_SIGNING_ACCOUNT', 'ARTIFACT_SIGNING_PROFILE', 'ARTIFACT_SIGNING_SUBJECT')) {
    if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) {
        throw "Missing signing configuration: $name"
    }
}
$file = (Resolve-Path -LiteralPath $Path).Path
Import-Module ArtifactSigning -RequiredVersion 0.1.8
# Only the Azure CLI session established by the workflow's OIDC login may sign.
$parameters = @{
    Endpoint = $env:ARTIFACT_SIGNING_ENDPOINT
    CodeSigningAccountName = $env:ARTIFACT_SIGNING_ACCOUNT
    CertificateProfileName = $env:ARTIFACT_SIGNING_PROFILE
    Files = $file
    FileDigest = 'SHA256'
    TimestampRfc3161 = 'http://timestamp.acs.microsoft.com'
    TimestampDigest = 'SHA256'
    ExcludeEnvironmentCredential = $true
    ExcludeWorkloadIdentityCredential = $true
    ExcludeManagedIdentityCredential = $true
    ExcludeSharedTokenCacheCredential = $true
    ExcludeVisualStudioCredential = $true
    ExcludeVisualStudioCodeCredential = $true
    ExcludeAzureCliCredential = $false
    ExcludeAzurePowerShellCredential = $true
    ExcludeAzureDeveloperCliCredential = $true
    ExcludeInteractiveBrowserCredential = $true
}
Invoke-ArtifactSigning @parameters
$signature = Get-AuthenticodeSignature -LiteralPath $file
if ($signature.Status -ne 'Valid' -or
    $signature.SignerCertificate.Subject -cne $env:ARTIFACT_SIGNING_SUBJECT -or
    -not $signature.TimeStamperCertificate) {
    throw "Signing verification failed for '$file': $($signature.Status)"
}

# Capture the exact patched payload before Tauri restores the portable binary.
if ($env:AZD_NSIS_PAYLOAD_RECEIPT -and [IO.Path]::GetFileName($file) -ceq 'azd-gui.exe') {
    if (Test-Path -LiteralPath $env:AZD_NSIS_PAYLOAD_RECEIPT) {
        throw 'NSIS payload receipt already exists; refusing stale or duplicate evidence.'
    }
    (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash |
        Set-Content -LiteralPath $env:AZD_NSIS_PAYLOAD_RECEIPT
}
