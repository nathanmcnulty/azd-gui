#Requires -Version 7.6

BeforeAll {
    $root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
    $selector = Join-Path $root 'scripts/get-release-attestation-files.ps1'
    function Get-FixtureHash($Path) {
        (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}

Describe 'Exact attestation subjects' {
    BeforeEach {
        $scratch = (Get-Item -LiteralPath TestDrive:\).FullName
        $directory = (New-Item -ItemType Directory -Path (Join-Path $scratch ([guid]::NewGuid().ToString('N')))).FullName
        $null = New-Item -ItemType Directory -Force -Path (Join-Path $directory 'bundle/nsis')
        $portable = Join-Path $directory 'azd-gui.exe'
        $installerName = 'Azure Deployment Studio_0.1.1_x64-setup.exe'
        $installer = Join-Path $directory 'bundle/nsis' $installerName
        $commit = '1' * 40
        $source = Join-Path $directory "azd-gui-$commit.tar.gz"
        $sums = Join-Path $directory 'SHA256SUMS.txt'
        $evidence = Join-Path $directory 'installed-artifact-verification.json'
        Set-Content -LiteralPath $portable -Value 'synthetic final portable bytes'
        Set-Content -LiteralPath $installer -Value 'synthetic final installer bytes'
        Set-Content -LiteralPath $source -Value 'synthetic archive bytes'
        $receipt = @{
            artifacts = @(
                @{ Name = 'azd-gui.exe'; Sha256 = (Get-FixtureHash $portable); SignatureStatus = 'Valid' }
                @{ Name = $installerName; Sha256 = (Get-FixtureHash $installer); SignatureStatus = 'Valid' }
            )
            installedExecutable = @{ signatureStatus = 'Valid' }
        }
        $receipt.artifacts | ForEach-Object { "$($_.Sha256)  $($_.Name)" } | Set-Content -LiteralPath $sums
        $receipt | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $evidence
        $selection = @{
            ReleaseDirectory = $directory; Commit = $commit
            EvidenceSha256 = (Get-FixtureHash $evidence); SourceSha256 = (Get-FixtureHash $source)
        }
    }

    It 'selects the final Windows files and exact source for attestation' {
        $files = & $selector @selection
        $files.Portable | Should -BeExactly $portable
        $files.Installer | Should -BeExactly $installer
        $files.Source | Should -BeExactly $source
        $files.Checksums | Should -BeExactly $sums
    }

    It 'rejects changed <Name> bytes before returning any subjects' -ForEach @(
        @{ Name = 'portable' }
        @{ Name = 'installer' }
        @{ Name = 'source' }
        @{ Name = 'evidence' }
    ) {
        Set-Content -LiteralPath (Get-Variable -Name $Name -ValueOnly) -Value 'different bytes'
        { & $selector @selection } | Should -Throw '*handoff hash*'
    }

    It 'rejects invalid publication checksums' {
        Set-Content -LiteralPath $sums -Value 'different manifest'
        { & $selector @selection } | Should -Throw '*differs from the verified Windows artifacts*'
    }
}

Describe 'Release-only workflow boundaries' {
    BeforeAll {
        $workflow = Get-Content -LiteralPath (Join-Path $root '.github/workflows/release.yml') -Raw
        $windows = [regex]::Match($workflow, '(?ms)^  windows:\s.*?(?=^  attest:)').Value
        $attest = [regex]::Match($workflow, '(?ms)^  attest:\s.*?(?=^  stage_release:)').Value
        $stage = [regex]::Match($workflow, '(?ms)^  stage_release:\s.*\z').Value
        $tagOnly = "if: github.repository == 'nathanmcnulty/azd-gui' && startsWith(github.ref, 'refs/tags/v')"
    }

    It 'keeps the existing version tag and confirmed manual entry points' {
        $workflow | Should -Match '(?m)^  push:\r?\n    tags:\r?\n      - "v\*"$'
        $workflow | Should -Match '(?m)^  workflow_dispatch:'
        $workflow | Should -Not -Match '(?m)^  (pull_request|pull_request_target|workflow_run|schedule|release):'
        $windows | Should -Match ([regex]::Escape("(!startsWith(github.ref, 'refs/tags/v') && inputs.confirm_release == true)"))
        $windows | Should -Match ([regex]::Escape("(github.event_name == 'push' || inputs.confirm_release == true)"))
        foreach ($job in @($attest, $stage)) { $job | Should -Match ([regex]::Escape($tagOnly)) }
    }

    It 'keeps attestation write permission separate from Azure and publication' {
        ([regex]::Matches($workflow, '(?m)^\s*attestations: write\s*$')).Count | Should -Be 1
        ([regex]::Matches($workflow, '(?m)^\s*id-token: write\s*$')).Count | Should -Be 2
        $windows | Should -Match 'environment: artifact-signing'
        $windows | Should -Not -Match '(attestations: write|contents: write|gh release)'
        $attest | Should -Match '(?m)^    needs: windows\r?$'
        $attest | Should -Match 'attestations: write'
        $attest | Should -Match 'id-token: write'
        $attest | Should -Not -Match '(azure/login|artifact-signing|contents: write|gh release)'
        $stage | Should -Match '(?m)^    needs: \[windows, attest\]\r?$'
        $stage | Should -Match 'gh release create(?s:.*?)--draft(?s:.*?)--verify-tag'
        $stage | Should -Not -Match '(id-token: write|attestations: write|azure/login|gh release publish)'
    }

    It 'requires tag and strict audit before signing and consumes one immutable artifact twice' {
        $windows | Should -Match 'confirm-release-tag\.ps1'
        $windows | Should -Match 'run: npm run audit:dependencies'
        $windows.IndexOf('- name: Verify signed release tag') |
            Should -BeLessThan $windows.IndexOf('- name: Sign in to Azure with GitHub OIDC')
        $windows.IndexOf('run: npm run audit:dependencies') |
            Should -BeLessThan $windows.IndexOf('- name: Sign in to Azure with GitHub OIDC')
        $windows.IndexOf('- name: Verify installed payload before upload') |
            Should -BeLessThan $windows.IndexOf('- name: Upload signed Windows artifacts')
        foreach ($job in @($attest, $stage)) {
            $job | Should -Match ([regex]::Escape('artifact-ids: ${{ needs.windows.outputs.artifact-id }}'))
            $job | Should -Match 'digest-mismatch: error'
            $job | Should -Match 'get-release-attestation-files\.ps1'
            $job | Should -Match 'confirm-release-tag\.ps1.*-ExpectedTagObject'
        }
        $attest | Should -Match 'uses: actions/attest@[0-9a-f]{40}'
        $attest | Should -Match 'steps.subjects.outputs.source'
        $attest | Should -Match 'steps.subjects.outputs.portable'
        $attest | Should -Match 'steps.subjects.outputs.installer'
    }
}
