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

function Get-PimPlannerFailureCode {
    param([System.Management.Automation.ErrorRecord] $Record)

    $fallback = 'PimPlannerRejectedConfiguration'
    try {
        $planner = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'AzdPim.Graph.psm1'))
        $lineNumber = $Record.InvocationInfo.ScriptLineNumber
        $fromPlanner = -not [string]::IsNullOrEmpty($Record.InvocationInfo.ScriptName) -and
            [IO.Path]::GetFullPath($Record.InvocationInfo.ScriptName) -ceq $planner
        # PowerShell locates some multiline-expression errors at a caller even
        # when that caller is inside the same module. Prefer only the first
        # engine-generated frame for these fixed functions and exact path.
        if ($Record.Exception -is [System.Management.Automation.PropertyNotFoundException]) {
            $frame = '\Aat (?:Resolve-AzdPimContext|Get-AzdPimReferencedContextIds|New-AzdPimPlan), ' + [regex]::Escape($planner) + ': line (?<line>[0-9]+)(?:\r?\n|\z)'
            $match = [regex]::Match([string]$Record.ScriptStackTrace, $frame)
            if ($match.Success) {
                $lineNumber = [int]$match.Groups['line'].Value
                $fromPlanner = $true
            }
        }
        if (-not $fromPlanner) { return $fallback }
        # Locations identify explicit throws in the exact embedded planner, not
        # tenant-controlled names, IDs or exception text. A changed pin must be
        # reviewed with this map; otherwise diagnostics remain safely generic.
        if ((Get-FileHash -LiteralPath $planner -Algorithm SHA256).Hash -ine 'de289c7566bd9449fe55ddcfbbf9c84d01ee8056095c4df0747df0beefac8947') { return $fallback }
        if ($Record.Exception -is [System.Management.Automation.PropertyNotFoundException]) {
            switch ($lineNumber) {
                { $_ -in @(283, 284, 285) } { return 'PimRoleReferenceDataInvalid' }
                { $_ -in @(452, 458, 467) } { return 'PimContextIdentityDataInvalid' }
                { $_ -in @(468, 469, 470, 471, 472) } { return 'PimContextSlotDataInvalid' }
                { $_ -in @(758, 759, 760, 761, 762, 763, 764, 765, 766) } { return 'PimRolePolicyDataInvalid' }
            }
            return 'PimPlannerPolicyDataInvalid'
        }
        switch ($lineNumber) {
            454 { return 'PimContextDuplicateNames' }
            458 { return 'PimContextAlreadyExists' }
            479 { return 'PimContextSlotsFull' }
            639 { return 'PimConditionalAccessDuplicateNames' }
            655 { return 'PimConditionalAccessAlreadyExists' }
            768 { return 'PimRoleContextConflict' }
            { $_ -in @(258, 264, 268, 273, 277) } { return 'PimConditionalAccessReferenceDataInvalid' }
            { $_ -in @(759, 761, 763, 765) } { return 'PimRolePolicyDataInvalid' }
            { $_ -in @(173, 178, 183, 501) } {
                return 'PimPlannerPolicyDataInvalid'
            }
        }
    } catch {
        # Diagnostic lookup can never export a path/error or replace the actual
        # Graph/MSAL classification performed by the caller below.
    }
    return $fallback
}

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
                $previewExistingObjects = $false
                foreach ($property in $request.parameters.PSObject.Properties) {
                    if ($property.Name -ceq 'previewExistingObjects') {
                        if ($property.Value -isnot [bool]) { throw 'PIM existing-object preview requires a boolean.' }
                        $previewExistingObjects = $property.Value
                        continue
                    }
                    if ($property.Name -notin @('privilegedRoleIds', 'lessPrivilegedRoleIds', 'privilegedAuthenticationProfile', 'lessPrivilegedAuthenticationProfile', 'privilegedDeviceRequirement', 'lessPrivilegedDeviceRequirement')) { throw 'Unsupported PIM configuration.' }
                    $configuration.($property.Name) = $property.Value
                }
                # These flags affect proposals only: the pinned read-only entry
                # point enforces plan mode, null state and the owned GET reader.
                $configuration.AdoptExisting = $previewExistingObjects
                $configuration.AdoptRoleContexts = $previewExistingObjects
                $owned.BeginPimPlanAudit($session, [long]$sequence)
                $plan = New-AzdPimReadPlan -Configuration $configuration -ReadContext $owned
                $plan | Add-Member NoteProperty previewExistingObjects $previewExistingObjects
                $plan | Add-Member NoteProperty ownershipReviewRequired $true
                $plan | Add-Member NoteProperty ownershipEstablished $false
                $plan | Add-Member NoteProperty executionAuthorized $false
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
            if ($request -and $request.action -eq 'pimPlan') { $code = Get-PimPlannerFailureCode -Record $_ }
            $exception = $_.Exception
            for ($depth = 0; $null -ne $exception -and $depth -lt 8; $depth++) {
                if ($exception -is [AzdGui.GraphAdapter.GraphBoundaryException]) {
                    if ($exception.Code -in @('GraphReadDenied', 'GraphReadBadRequest', 'GraphReadNotFound', 'GraphReadThrottled', 'GraphDelegatedScopeMissing', 'GraphIdentityMismatch', 'GraphIdentityOrGrantsChanged', 'PimVerificationBudgetExceeded', 'PimReadBudgetOrSessionRejected', 'PimResponseLimitExceeded', 'GraphRedirectRejected', 'GraphReadFailed')) { $code = $exception.Code }
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
