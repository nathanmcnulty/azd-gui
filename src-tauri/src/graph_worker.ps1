param([string] $AdapterModule = (Join-Path $PSScriptRoot 'GraphAdapter.psm1'))
# One credential and its protected reads live in this PowerShell process.
# Only whitelisted metadata crosses stdout. Tokens stay inside managed code.
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
$binding = $null
$session = $null
$owned = $null
$sequence = 0
$connected = $false
try {
    Import-Module $AdapterModule -ErrorAction Stop
    while ($null -ne ($line = [Console]::ReadLine())) {
        $request = $null
        $plan = $null
        try {
            if ($line.Length -gt 65536) { throw 'Oversized request.' }
            $request = $line | ConvertFrom-Json -Depth 16
            if ($request.sequence -ne ($sequence + 1)) { throw 'Stale sequence.' }
            $sequence = $request.sequence
            if ($request.action -eq 'initialize') {
                if ($null -ne $binding) { throw 'Fresh process required.' }
                $binding = $request.binding
                $session = [string]$request.sessionId
                $owned = New-OwnedGraphSession -BindingJson ($binding | ConvertTo-Json -Depth 12 -Compress)
            }
            elseif ($null -eq $binding -or $request.sessionId -cne $session -or
                $request.requirementsDigest -cne $binding.requirementsDigest) { throw 'Session binding mismatch.' }
            elseif ($request.action -eq 'authenticate') {
                if ($connected -or $request.allowInteractive -ne $true) { throw 'Explicit sign-in approval required.' }
                $owned.Authenticate()
                $connected = $true
            }
            elseif ($request.action -eq 'probe') {
                if (-not $connected) { throw 'No owned authenticated session.' }
                if ($binding.PSObject.Properties['purpose'] -and $binding.purpose -ceq 'pimPlan') { $owned.BeginPimPlanAudit($session, [long]$sequence) }
                $owned.Probe()
            }
            elseif ($request.action -eq 'pimPlan') {
                if (-not $connected) { throw 'No owned authenticated session.' }
                $requiredScopes = @('AuthenticationContext.Read.All', 'Policy.Read.All', 'RoleManagement.Read.Directory')
                if ($binding.purpose -cne 'pimPlan') { throw 'PIM session purpose required.' }
                if ((($binding.scopes | Sort-Object) -join ',') -cne ($requiredScopes -join ',')) { throw 'PIM read-only scope contract mismatch.' }
                # These modules are embedded from the pinned public template by
                # the native host. No renderer-selected script or hook is run.
                Import-Module (Join-Path $PSScriptRoot 'AzdPim.Common.psm1') -Force
                Import-Module (Join-Path $PSScriptRoot 'AzdPim.Graph.psm1') -Force
                Get-ChildItem Env:AZD_PIM_* | Remove-Item
                $configuration = Get-AzdPimConfiguration
                foreach ($property in $request.parameters.PSObject.Properties) {
                    if ($property.Name -notin @('privilegedRoleIds', 'lessPrivilegedRoleIds', 'privilegedAuthenticationProfile', 'lessPrivilegedAuthenticationProfile', 'privilegedDeviceRequirement', 'lessPrivilegedDeviceRequirement')) { throw 'Unsupported PIM configuration.' }
                    $configuration.($property.Name) = $property.Value
                }
                $owned.BeginPimPlanAudit($session, [long]$sequence)
                $plan = New-AzdPimReadPlan -Configuration $configuration -ReadContext $owned
                $planJson = $plan | ConvertTo-Json -Depth 100 -Compress
                if ([Text.Encoding]::UTF8.GetByteCount($planJson) -gt 8MB) { throw 'PIM plan artifact exceeds limit.' }
            }
            elseif ($request.action -eq 'dispose') { break }
            else { throw 'Unsupported Graph operation.' }
            $evidence = if ($connected) { $owned.EvidenceJson() | ConvertFrom-Json -Depth 12 } else { $null }
            $reply = [ordered]@{ sequence=$sequence; sessionId=$session; requirementsDigest=$binding.requirementsDigest; processId=$PID; action=[string]$request.action; ok=$true; evidence=$evidence }
            if ($null -ne $plan) { $reply.pimPlan = $plan }
            [Console]::Out.WriteLine('AZD_GRAPH_RPC:' + ($reply | ConvertTo-Json -Depth 100 -Compress))
            [Console]::Out.Flush()
        }
        catch {
            # Never serialize raw SDK errors, AuthenticationResult or HTTP data.
            $code = if ($request -and $request.action -eq 'authenticate') { 'GraphAuthenticationFailed' } elseif ($request -and $request.action -eq 'pimPlan') { 'PimPlannerRejectedConfiguration' } else { 'PimReadFailed' }
            $exception = $_.Exception
            for ($depth = 0; $null -ne $exception -and $depth -lt 8; $depth++) {
                if ($exception -is [AzdGui.GraphAdapter.GraphBoundaryException]) {
                    if ($exception.Code -in @('GraphReadDenied', 'GraphDelegatedScopeMissing', 'GraphIdentityMismatch', 'GraphIdentityOrGrantsChanged', 'PimVerificationBudgetExceeded', 'PimReadBudgetOrSessionRejected', 'PimResponseLimitExceeded', 'GraphRedirectRejected', 'GraphReadFailed')) { $code = $exception.Code }
                    else { $code = 'PimReadFailed' }
                    break
                }
                if ($exception -is [System.Text.Json.JsonException] -or $exception -is [System.OperationCanceledException]) { $code = 'PimReadFailed'; break }
                if ($exception -is [Microsoft.Identity.Client.MsalUiRequiredException]) { $code = 'GraphInteractionRequired'; break }
                if ($exception -is [Microsoft.Identity.Client.MsalException]) { if ($request.action -ne 'authenticate') { $code = 'GraphTokenRefreshFailed' }; break }
                if ($exception -is [System.Net.Http.HttpRequestException]) { $code = 'GraphReadFailed'; break }
                $exception = $exception.InnerException
            }
            $reply = [ordered]@{ sequence=$sequence; sessionId=$session; requirementsDigest=if($binding){$binding.requirementsDigest}else{''}; processId=$PID; action=if($request){[string]$request.action}else{''}; ok=$false; evidence=$null }
            $reply.errorCode = $code
            [Console]::Out.WriteLine('AZD_GRAPH_RPC:' + ($reply | ConvertTo-Json -Depth 12 -Compress))
            [Console]::Out.Flush()
            exit 1
        }
    }
}
finally {
    if ($null -ne $owned) { $owned.Dispose() }
    $owned = $null
    $binding = $null
    $session = $null
    $connected = $false
}
