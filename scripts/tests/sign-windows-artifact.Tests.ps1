#Requires -Version 7.6

BeforeAll {
    # Non-Windows hosts need only a mock target, never an Authenticode fallback.
    # Leave the real Windows command intact; an unmocked call must fail closed.
    if (-not (Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue)) {
        function Get-AuthenticodeSignature {
            param([string] $LiteralPath)
            throw 'Offline test shim must be mocked; no Authenticode verification occurred.'
        }
    }

    $signer = Join-Path $PSScriptRoot '../sign-windows-artifact.ps1'
    # Signing is mocked: these tests never request Azure credentials or signatures.
    function Invoke-ArtifactSigning {
        param($Endpoint, $CodeSigningAccountName, $CertificateProfileName, $Files,
            $FileDigest, $TimestampRfc3161, $TimestampDigest,
            $ExcludeEnvironmentCredential, $ExcludeWorkloadIdentityCredential,
            $ExcludeManagedIdentityCredential, $ExcludeSharedTokenCacheCredential,
            $ExcludeVisualStudioCredential, $ExcludeVisualStudioCodeCredential,
            $ExcludeAzureCliCredential, $ExcludeAzurePowerShellCredential,
            $ExcludeAzureDeveloperCliCredential, $ExcludeInteractiveBrowserCredential)
    }
    $names = @('ARTIFACT_SIGNING_ENDPOINT', 'ARTIFACT_SIGNING_ACCOUNT', 'ARTIFACT_SIGNING_PROFILE', 'ARTIFACT_SIGNING_SUBJECT', 'AZD_NSIS_PAYLOAD_RECEIPT')
    $original = @{}
    foreach ($name in $names) { $original[$name] = [Environment]::GetEnvironmentVariable($name) }
}

AfterAll {
    foreach ($name in $names) { [Environment]::SetEnvironmentVariable($name, $original[$name]) }
}

Describe 'Tauri signing hook' {
    BeforeEach {
        $env:ARTIFACT_SIGNING_ENDPOINT = 'https://example.invalid'
        $env:ARTIFACT_SIGNING_ACCOUNT = 'test'
        $env:ARTIFACT_SIGNING_PROFILE = 'test'
        $env:ARTIFACT_SIGNING_SUBJECT = 'CN=Test'
        $env:AZD_NSIS_PAYLOAD_RECEIPT = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
        $artifact = Join-Path $TestDrive 'azd-gui.exe'
        Set-Content $artifact 'test fixture'
        Mock Import-Module {}
        Mock Invoke-ArtifactSigning {}
        Mock Get-AuthenticodeSignature {
            [pscustomobject]@{
                Status = 'Valid'
                SignerCertificate = [pscustomobject]@{ Subject = 'CN=Test' }
                TimeStamperCertificate = [pscustomobject]@{ Subject = 'CN=Timestamp' }
            }
        }
    }

    It 'records the verified payload and uses only the Azure CLI credential' {
        & $signer -Path $artifact
        (Get-Content $env:AZD_NSIS_PAYLOAD_RECEIPT -Raw).Trim() | Should -Be (Get-FileHash $artifact -Algorithm SHA256).Hash
        Should -Invoke Invoke-ArtifactSigning -Times 1 -Exactly -ParameterFilter {
            $ExcludeEnvironmentCredential -and $ExcludeWorkloadIdentityCredential -and
            $ExcludeManagedIdentityCredential -and $ExcludeSharedTokenCacheCredential -and
            $ExcludeVisualStudioCredential -and $ExcludeVisualStudioCodeCredential -and
            -not $ExcludeAzureCliCredential -and $ExcludeAzurePowerShellCredential -and
            $ExcludeAzureDeveloperCliCredential -and $ExcludeInteractiveBrowserCredential -and
            $FileDigest -eq 'SHA256' -and $TimestampDigest -eq 'SHA256'
        }
        Should -Invoke Import-Module -Times 1 -ParameterFilter { $RequiredVersion -eq '0.1.8' }
    }

    It 'rejects a service failure without a receipt' {
        Mock Invoke-ArtifactSigning { throw 'Service failure' }
        { & $signer -Path $artifact } | Should -Throw '*Service failure*'
        Test-Path $env:AZD_NSIS_PAYLOAD_RECEIPT | Should -BeFalse
    }

    It 'rejects an invalid signature without a receipt' {
        Mock Get-AuthenticodeSignature { [pscustomobject]@{ Status = 'HashMismatch' } }
        { & $signer -Path $artifact } | Should -Throw '*Signing verification failed*'
        Test-Path $env:AZD_NSIS_PAYLOAD_RECEIPT | Should -BeFalse
    }

    It 'rejects stale payload evidence' {
        Set-Content $env:AZD_NSIS_PAYLOAD_RECEIPT 'stale'
        { & $signer -Path $artifact } | Should -Throw '*receipt already exists*'
    }
}
