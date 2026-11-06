# Load only the native application's embedded, version-pinned managed adapter.
# No installed Graph module, shared authrecord, token cache or user profile loads.
Add-Type -Path (Join-Path $PSScriptRoot 'AzdGui.GraphAdapter.dll')
[AzdGui.GraphAdapter.Bootstrap]::LoadDependencies($PSScriptRoot)
function New-OwnedGraphSession {
    param([Parameter(Mandatory)][string]$BindingJson)
    return [AzdGui.GraphAdapter.Bootstrap]::Create($BindingJson)
}
Export-ModuleMember -Function New-OwnedGraphSession
