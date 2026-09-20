BeforeAll {
    $verifier = Join-Path $PSScriptRoot '../verify-release-artifacts.ps1'
}

Describe 'Release artifact version verification' {
    BeforeEach {
        $release = Join-Path $TestDrive ([guid]::NewGuid().ToString('N'))
        New-Item -ItemType Directory -Path $release | Out-Null
        $names = @('azd-gui.exe', 'Azure Deployment Studio_0.1.0_x64-setup.exe')
        $checksums = foreach ($name in $names) {
            $path = Join-Path $release $name
            Set-Content -LiteralPath $path -Value 'test fixture, not an executable'
            "$((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant())  $name"
        }
        Set-Content -LiteralPath (Join-Path $release 'SHA256SUMS.txt') -Value $checksums
        Mock Get-AuthenticodeSignature {
            [pscustomobject]@{
                Status = [System.Management.Automation.SignatureStatus]::Valid
                SignerCertificate = [pscustomobject]@{ Subject = 'CN=Test'; Thumbprint = 'test' }
                TimeStamperCertificate = [pscustomobject]@{ Subject = 'CN=Timestamp' }
            }
        }
    }

    It 'accepts the exact requested version and records evidence' {
        Mock Get-Item { [pscustomobject]@{ VersionInfo = @{ ProductVersion = '0.1.0' } } }
        & $verifier -ReleaseDirectory $release -ExpectedVersion '0.1.0' -ExpectedSignerSubject 'CN=Test'
        $evidence = Get-Content (Join-Path $release 'artifact-verification.json') -Raw | ConvertFrom-Json
        $evidence.expectedVersion | Should -BeExactly '0.1.0'
        $evidence.artifacts.Count | Should -Be 2
    }

    It 'rejects mismatched version <Actual> without writing evidence' -ForEach @(
        @{ Actual = '0.1.01' }
        @{ Actual = '0.1.0-beta' }
        @{ Actual = '0.1.0+different-build' }
        @{ Actual = '0.1.0.1' }
        @{ Actual = '' }
    ) {
        Mock Get-Item { [pscustomobject]@{ VersionInfo = @{ ProductVersion = $Actual } } }
        { & $verifier -ReleaseDirectory $release -ExpectedVersion '0.1.0' -ExpectedSignerSubject 'CN=Test' } |
            Should -Throw '*does not match*'
        Test-Path (Join-Path $release 'artifact-verification.json') | Should -BeFalse
    }

    It 'verifies a separately signed installed payload with a different hash' {
        Mock Get-Item { [pscustomobject]@{ VersionInfo = @{ ProductVersion = '0.1.0' } } }
        $installed = Join-Path $TestDrive 'installed.exe'
        Set-Content -LiteralPath $installed -Value 'different NSIS marker and signature'
        & $verifier -ReleaseDirectory $release -ExpectedVersion '0.1.0' -ExpectedSignerSubject 'CN=Test' -InstalledExecutablePath $installed
        $evidence = Get-Content (Join-Path $release 'artifact-verification.json') -Raw | ConvertFrom-Json
        $evidence.installedExecutable.signatureStatus | Should -BeExactly 'Valid'
        $evidence.installedExecutable.sha256 | Should -Not -Be $evidence.artifacts[0].Sha256
    }

    It 'rejects a modified installed executable even when outer artifacts are valid' {
        Mock Get-Item { [pscustomobject]@{ VersionInfo = @{ ProductVersion = '0.1.0' } } }
        $installed = Join-Path $TestDrive 'installed.exe'
        Set-Content -LiteralPath $installed -Value 'modified after signing'
        Mock Get-AuthenticodeSignature {
            [pscustomobject]@{ Status = [System.Management.Automation.SignatureStatus]::HashMismatch }
        } -ParameterFilter { $LiteralPath -like '*installed.exe' }
        { & $verifier -ReleaseDirectory $release -ExpectedVersion '0.1.0' -ExpectedSignerSubject 'CN=Test' -InstalledExecutablePath $installed } |
            Should -Throw '*Invalid installed executable signature: HashMismatch*'
        Test-Path (Join-Path $release 'artifact-verification.json') | Should -BeFalse
    }

    It 'rejects an installed executable with a different version' {
        Mock Get-Item { [pscustomobject]@{ VersionInfo = @{ ProductVersion = '0.1.0' } } }
        $installed = Join-Path $TestDrive 'installed.exe'
        Set-Content -LiteralPath $installed -Value 'old version'
        Mock Get-Item { [pscustomobject]@{ VersionInfo = @{ ProductVersion = '0.0.9' } } } -ParameterFilter { $LiteralPath -like '*installed.exe' }
        { & $verifier -ReleaseDirectory $release -ExpectedVersion '0.1.0' -ExpectedSignerSubject 'CN=Test' -InstalledExecutablePath $installed } |
            Should -Throw '*Installed executable version*does not match*'
        Test-Path (Join-Path $release 'artifact-verification.json') | Should -BeFalse
    }
}
