param(
    [Parameter(Mandatory)][string]$BuildRoot,
    [Parameter(Mandatory)][string]$PlannerRoot
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
$managed = Join-Path $BuildRoot 'managed'
dotnet --version
pwsh --version
dotnet publish (Join-Path $repository 'src-tauri/graph-adapter-tests/AzdGui.GraphAdapter.Tests.csproj') --configuration Release --output $managed -p:RestoreLockedMode=true --nologo
if ($LASTEXITCODE -ne 0) { throw 'Managed test adapter build failed.' }
Add-Type -Path (Join-Path $managed 'AzdGui.GraphAdapter.dll')
[AzdGui.GraphAdapter.Bootstrap]::LoadDependencies($managed)
Add-Type -Path (Join-Path $managed 'AzdGui.GraphAdapter.Tests.dll')
$boundaryChecks = [AzdGui.GraphAdapter.Tests.PimReaderChecks]::Run((Join-Path $BuildRoot 'reader-boundaries'))
Write-Host "Managed PIM reader negative boundary checks passed: $boundaryChecks"
Import-Module (Join-Path $PlannerRoot 'scripts/AzdPim.Common.psm1') -Force
Import-Module (Join-Path $PlannerRoot 'scripts/AzdPim.Graph.psm1') -Force
$fixtures = Join-Path $BuildRoot ([guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $fixtures | Out-Null
$control = Join-Path $fixtures 'control.json'
@{ account = 'operator@example.test'; mode = 'pim'; delayMs = 0 } | ConvertTo-Json | Set-Content $control
$binding = @{
    requirementsDigest = 'offline-pim-fixture'
    purpose = 'pimPlan'
    account = 'operator@example.test'
    tenantId = '11111111-1111-4111-8111-111111111111'
    clientId = '22222222-2222-4222-8222-222222222222'
    resource = 'https://graph.microsoft.com'
    scopes = @('RoleManagement.Read.Directory', 'AuthenticationContext.Read.All', 'Policy.Read.All')
    probeUri = 'https://graph.microsoft.com/v1.0/identity/conditionalAccess/policies?$top=1&$select=id'
} | ConvertTo-Json -Compress
$role = '33333333-3333-4333-8333-333333333333'
$inventoryUri = 'https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20true'
$continuation = $inventoryUri + '&$skiptoken=offline-fixture'
$responses = @{
    $inventoryUri = @{ value = @(); '@odata.nextLink' = $continuation }
    $continuation = @{ value = @(@{ id = $role; displayName = 'Offline privileged role'; description = ''; isBuiltIn = $true; isEnabled = $true; isPrivileged = $true }) }
    'https://graph.microsoft.com/beta/roleManagement/directory/roleDefinitions?$filter=isPrivileged%20eq%20false' = @{ value = @() }
    'https://graph.microsoft.com/v1.0/policies/roleManagementPolicyAssignments?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$top=999' = @{ value = @(@{ roleDefinitionId = $role; policyId = 'fixture-policy' }) }
    'https://graph.microsoft.com/v1.0/policies/roleManagementPolicies?$filter=scopeId%20eq%20%27%2F%27%20and%20scopeType%20eq%20%27DirectoryRole%27&$expand=rules&$top=999' = @{ value = @(@{ id = 'fixture-policy'; rules = @(
        @{ id = 'AuthenticationContext_EndUser_Assignment'; isEnabled = $false; claimValue = '' },
        @{ id = 'Enablement_EndUser_Assignment'; enabledRules = @('MultiFactorAuthentication', 'Justification') }
    ) }) }
    'https://graph.microsoft.com/v1.0/identity/conditionalAccess/authenticationContextClassReferences' = @{ value = @() }
    'https://graph.microsoft.com/beta/identity/conditionalAccess/policies?$top=999' = @{ value = @() }
}
$responses | ConvertTo-Json -Depth 30 | Set-Content (Join-Path $fixtures 'responses.json')
$saved = @{}
Get-ChildItem Env:AZD_PIM_* | ForEach-Object { $saved[$_.Name] = $_.Value; Remove-Item -LiteralPath "Env:$($_.Name)" }
$owned = $null
try {
    $configuration = Get-AzdPimConfiguration
    $configuration.PrivilegedRoleIds = @($role)
    $owned = [AzdGui.GraphAdapter.Tests.TestFactory]::Create($binding, $control)
    $owned.Authenticate()
    $owned.Probe()
    $plan = New-AzdPimReadPlan -Configuration $configuration -ReadContext $owned
    if ($plan.tiers.privileged.roles.Count -ne 1 -or $plan.tiers.privileged.context.action -ne 'create' -or
        $plan.tiers.privileged.conditionalAccessPolicies[0].action -ne 'create') { throw 'Real planner did not produce expected proposals.' }
    Write-AzdPimPlanReport -Plan $plan -Path (Join-Path $fixtures 'plan.json')
    $calls = @(Get-Content (Join-Path $fixtures 'calls.jsonl') | ConvertFrom-Json)
    $reads = @($calls | Where-Object kind -eq 'protected-read-start')
    if ($reads.Count -ne 8 -or @($calls.pid | Sort-Object -Unique).Count -ne 1 -or $calls[0].pid -ne $PID) { throw 'Planner reads escaped the owning process or missed pagination.' }
    try { $owned.ReadPimJson('https://graph.microsoft.com/v1.0/users'); throw 'Unregistered endpoint accepted.' }
    catch { if ($_.Exception.Message -notlike '*UnregisteredPimRead*') { throw } }
    Write-Host 'Real planner, managed credential validation, same-process GETs, pagination and endpoint rejection passed without cloud access.'
} finally {
    if ($null -ne $owned) { $owned.Dispose() }
    Get-ChildItem Env:AZD_PIM_* | Remove-Item
    foreach ($name in $saved.Keys) { Set-Item -LiteralPath "Env:$name" -Value $saved[$name] }
}
