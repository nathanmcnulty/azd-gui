#Requires -Version 7.6

BeforeAll {
    $root = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
    $runner = Join-Path $root 'scripts/test-powershell.ps1'
}

Describe 'Owned PowerShell tooling baseline' {
    It 'requires PowerShell 7.6 before executing <Name>' -ForEach @(
        @{ Name = 'assert-powershell-version.ps1' }
        @{ Name = 'test-powershell.ps1' }
        @{ Name = 'sign-windows-artifact.ps1' }
        @{ Name = 'verify-release-artifacts.ps1' }
    ) {
        $tokens = $null
        $parseErrors = $null
        $ast = [System.Management.Automation.Language.Parser]::ParseFile(
            (Join-Path $root "scripts/$Name"), [ref] $tokens, [ref] $parseErrors)
        $parseErrors.Count | Should -Be 0
        $ast.ScriptRequirements.RequiredPSVersion | Should -Be ([version] '7.6')
    }

    It 'keeps current catalog PowerShell prerequisites at 7.6' {
        $found = 0
        foreach ($file in Get-ChildItem (Join-Path $root 'catalog/manifests/*.json')) {
            $manifest = Get-Content $file.FullName -Raw | ConvertFrom-Json
            foreach ($requirement in $manifest.prerequisites | Where-Object id -EQ 'pwsh') {
                $requirement.minimumVersion | Should -BeExactly '7.6.0'
                $found++
            }
        }
        $found | Should -Be 4
    }

    It 'asserts the engine and pins Pester before existing validation in <Name>' -ForEach @(
        @{ Name = 'ci.yml'; FirstStep = '- name: Install frontend dependencies' }
        @{ Name = 'release.yml'; FirstStep = '- name: Verify application and tag versions agree' }
    ) {
        $workflow = Get-Content (Join-Path $root ".github/workflows/$Name") -Raw
        $assertion = $workflow.IndexOf('run: ./scripts/assert-powershell-version.ps1')
        $assertion | Should -BeGreaterThan -1
        $assertion | Should -BeLessThan $workflow.IndexOf($FirstStep)
        $workflow | Should -Match 'Install-Module Pester -RequiredVersion 6\.2\.0 '
        $workflow | Should -Match 'run: npm run test:powershell'
        $workflow.IndexOf('Install-Module Pester') | Should -BeLessThan $workflow.IndexOf('run: npm run test:powershell')
        if ($Name -eq 'release.yml') {
            $workflow.IndexOf('run: npm run test:powershell') |
                Should -BeLessThan $workflow.IndexOf('- name: Sign in to Azure with GitHub OIDC')
            $workflow.IndexOf('run: npm run test:powershell') |
                Should -BeLessThan $workflow.IndexOf('- name: Install pinned signing module')
        }
    }

    It 'retains manual-only hosted development validation' {
        $workflow = Get-Content (Join-Path $root '.github/workflows/ci.yml') -Raw
        $workflow | Should -Match '(?m)^  workflow_dispatch:'
        $workflow | Should -Not -Match '(?m)^  (push|pull_request|schedule):'
    }
}

Describe 'Pinned Pester entry point' {
    BeforeEach {
        Mock Import-Module { [pscustomobject]@{ Version = [version] '6.2.0' } }
        $runResult = [pscustomobject]@{
            Result = 'Passed'; TotalCount = 13; PassedCount = 13; FailedCount = 0
            SkippedCount = 0; InconclusiveCount = 0; NotRunCount = 0
        }
        Mock Invoke-Pester { $runResult }
    }

    It 'selects exactly Pester 6.2.0 and runs the maintained suite from any directory' {
        Push-Location $TestDrive
        try { & $runner } finally { Pop-Location }
        Should -Invoke Import-Module -Times 1 -Exactly -ParameterFilter {
            $Name -eq 'Pester' -and $RequiredVersion -eq '6.2.0' -and $PassThru -and $ErrorAction -eq 'Stop'
        }
        Should -Invoke Invoke-Pester -Times 1 -Exactly -ParameterFilter {
            $Path -eq (Join-Path $root 'scripts/tests') -and $PassThru -and $Output -eq 'Detailed'
        }
    }

    It 'fails without starting tests if the pinned module is unavailable' {
        Mock Import-Module { throw 'Pinned Pester is unavailable' }
        { & $runner } | Should -Throw '*Pinned Pester is unavailable*'
        Should -Invoke Invoke-Pester -Times 0 -Exactly
    }

    It 'rejects <Case> instead of reporting successful validation' -ForEach @(
        @{ Case = 'a failed result'; Property = 'Result'; Value = 'Failed' }
        @{ Case = 'zero discovered tests'; Property = 'TotalCount'; Value = 0 }
        @{ Case = 'a failing test'; Property = 'FailedCount'; Value = 1 }
        @{ Case = 'an incomplete pass count'; Property = 'PassedCount'; Value = 12 }
        @{ Case = 'a skipped test'; Property = 'SkippedCount'; Value = 1 }
        @{ Case = 'an inconclusive test'; Property = 'InconclusiveCount'; Value = 1 }
        @{ Case = 'a test that did not run'; Property = 'NotRunCount'; Value = 1 }
    ) {
        $runResult.$Property = $Value
        { & $runner } | Should -Throw '*must all run and pass*'
    }
}
