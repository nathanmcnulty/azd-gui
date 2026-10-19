#Requires -Version 7.6

[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# Install explicitly during development/CI setup, never implicitly in the runner.
$pester = Import-Module Pester -RequiredVersion 6.2.0 -PassThru -ErrorAction Stop
Write-Host "Offline tooling tests: PowerShell $($PSVersionTable.PSVersion); Pester $($pester.Version)"
$result = Invoke-Pester -Path (Join-Path $PSScriptRoot 'tests') -Output Detailed -PassThru
Write-Host "Tests: total=$($result.TotalCount), passed=$($result.PassedCount), failed=$($result.FailedCount), skipped=$($result.SkippedCount), inconclusive=$($result.InconclusiveCount), notRun=$($result.NotRunCount)"
if ($result.Result -ne 'Passed' -or $result.TotalCount -le 0 -or
    $result.PassedCount -ne $result.TotalCount -or $result.FailedCount -ne 0 -or
    $result.SkippedCount -ne 0 -or $result.InconclusiveCount -ne 0 -or $result.NotRunCount -ne 0) {
    throw 'PowerShell tooling tests must all run and pass; empty, skipped, or failed runs are not validation.'
}
