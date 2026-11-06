# Test-only MSAL/network substitutes, hosted inside the REAL managed adapter.
Add-Type -Path (Join-Path $PSScriptRoot 'AzdGui.GraphAdapter.dll')
[AzdGui.GraphAdapter.Bootstrap]::LoadDependencies($PSScriptRoot)
Add-Type -Path (Join-Path $PSScriptRoot 'AzdGui.GraphAdapter.Tests.dll')
function New-OwnedGraphSession {
    param([Parameter(Mandatory)][string]$BindingJson)
    return [AzdGui.GraphAdapter.Tests.TestFactory]::Create($BindingJson, (Join-Path $PSScriptRoot 'control.json'))
}
Export-ModuleMember -Function New-OwnedGraphSession
