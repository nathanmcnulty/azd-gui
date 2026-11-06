# Dot-sourced only by the embedded worker. No renderer-selected scripts run.
$pilotReview = $null
$pilotLock = $null
$pilotDirectory = $null

function ConvertTo-PilotCanonical {
    param([AllowNull()] $Value)
    if ($null -eq $Value) { return 'null' }
    if ($Value -is [System.Collections.IDictionary]) {
        $parts = foreach ($key in @($Value.Keys | Sort-Object -CaseSensitive)) {
            (ConvertTo-Json -InputObject ([string]$key) -Compress) + ':' + (ConvertTo-PilotCanonical $Value[$key])
        }
        return '{' + ($parts -join ',') + '}'
    }
    if ($Value -is [System.Collections.IEnumerable] -and $Value -isnot [string]) {
        $parts = foreach ($item in $Value) { ConvertTo-PilotCanonical $item }
        return '[' + ($parts -join ',') + ']'
    }
    return ConvertTo-Json -InputObject $Value -Compress -Depth 100
}
function Get-PilotDigest {
    param($Plan, $State, $Configuration)
    $body = $Plan | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100
    $body.Remove('generatedAt')
    $identity = $owned.EvidenceJson() | ConvertFrom-Json
    $json = ConvertTo-PilotCanonical @{
        plan = $body; state = $State | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100
        configuration = $Configuration | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100
        binding = $binding | ConvertTo-Json -Depth 100 | ConvertFrom-Json -AsHashtable -Depth 100
        principal = $identity.principalFingerprint; projectEnvironment = $pilotDirectory; session = $session
    }
    return [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes($json))).ToLowerInvariant()
}
function Write-PilotJson {
    param([string]$Path, $Value)
    $temporary = "$Path.$([guid]::NewGuid()).tmp"
    [IO.File]::WriteAllText($temporary, ($Value | ConvertTo-Json -Depth 100), [Text.UTF8Encoding]::new($false))
    [IO.File]::Move($temporary, $Path, $true)
}
function Read-PilotState {
    $path = Join-Path $pilotDirectory 'azd-pim-state.json'
    if ([IO.File]::Exists($path)) {
        if ((Get-Item -LiteralPath $path).Length -gt 16MB) { throw 'Pilot state exceeds limit.' }
        return Get-Content -LiteralPath $path -Raw | ConvertFrom-Json -Depth 100
    }
    return $null
}
function Invoke-PilotOperation {
    param($Parameters, [bool]$Apply)
    if (-not $connected -or $binding.purpose -cne 'pimPilot') { throw 'No owned pilot session.' }
    Import-Module (Join-Path $PSScriptRoot 'AzdPim.Common.psm1') -Force
    Import-Module (Join-Path $PSScriptRoot 'AzdPim.Graph.psm1') -Force
    if ($null -eq $script:pilotDirectory) {
        $script:pilotDirectory = [IO.Path]::GetFullPath([string]$Parameters.directory)
        # Native registration/path checks precede this. Recheck every existing
        # ancestor before opening the environment lock and durable files.
        $cursor = $script:pilotDirectory
        while ($cursor) {
            if ((Test-Path -LiteralPath $cursor) -and ((Get-Item -LiteralPath $cursor -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Aliased pilot directory.' }
            $cursor = Split-Path -Parent $cursor
        }
        [IO.Directory]::CreateDirectory($script:pilotDirectory) | Out-Null
        $script:pilotLock = [IO.File]::Open((Join-Path $script:pilotDirectory 'azd-pim-gui.lock'), 'OpenOrCreate', 'ReadWrite', 'None')
    }
    if ([IO.Path]::GetFullPath([string]$Parameters.directory) -cne $script:pilotDirectory) { throw 'Project/environment changed.' }
    foreach ($file in Get-ChildItem -LiteralPath $pilotDirectory -Force) {
        if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Aliased pilot file.' }
    }
    # A failed or cancelled first inventory read must not lose selected controls.
    Write-PilotJson (Join-Path $pilotDirectory 'azd-pim-gui-draft.json') @{
        identity = @{ account = $binding.account; tenantId = $binding.tenantId; clientId = $binding.clientId }
        configuration = $Parameters.configuration; approvalRestorable = $false
    }
    Get-ChildItem Env:AZD_PIM_* | Remove-Item
    $configuration = Get-AzdPimConfiguration
    foreach ($property in $Parameters.configuration.PSObject.Properties) {
        if ($property.Name -ceq 'previewExistingObjects') {
            if ($property.Value -isnot [bool]) { throw 'Invalid adoption preview.' }
            $configuration.AdoptExisting = $property.Value
            $configuration.AdoptRoleContexts = $property.Value
        } elseif ($property.Name -cin @('privilegedRoleIds', 'lessPrivilegedRoleIds', 'privilegedAuthenticationProfile', 'lessPrivilegedAuthenticationProfile', 'privilegedDeviceRequirement', 'lessPrivilegedDeviceRequirement')) {
            $configuration.($property.Name) = $property.Value
        } else { throw 'Unsupported pilot configuration.' }
    }
    $owned.BeginPimPlanAudit($session, [long]$sequence)
    $state = Read-PilotState
    # Complete metadata is re-read in addition to the planner's classification.
    $roles = @($owned.ReadRoleCatalogJson() | ConvertFrom-Json)
    foreach ($selection in @(
        @{ ids = $configuration.PrivilegedRoleIds; privileged = $true },
        @{ ids = $configuration.LessPrivilegedRoleIds; privileged = $false }
    )) {
        foreach ($id in $selection.ids) {
            $found = @($roles | Where-Object id -eq $id)
            if ($found.Count -ne 1 -or $found[0].isEnabled -ne $true -or $found[0].isPrivileged -ne $selection.privileged) { throw 'Selected role changed or is unavailable.' }
        }
    }
    $fresh = New-AzdPimOwnedPlan -Configuration $configuration -Context $owned -State $state
    $selected = @($configuration.PrivilegedRoleIds) + @($configuration.LessPrivilegedRoleIds)
    $blockers = [Collections.Generic.List[string]]::new()
    if ($state) {
        $pendingRoles = $state.PSObject.Properties['pendingRoleRules']
        if ($pendingRoles) { foreach ($property in $pendingRoles.Value.PSObject.Properties) {
            if ($property.Name -notin $selected) { $blockers.Add("Pending role $($property.Name) is outside the selection. Restore that role to its original tier and recheck before resuming.") }
        } }
        foreach ($tierName in @('privileged', 'lessPrivileged')) {
            $section = $state.PSObject.Properties['contexts']
            $entry = if ($section) { $section.Value.PSObject.Properties[$tierName] } else { $null }
            if ($entry -and $entry.Value.PSObject.Properties['pendingIntent'] -and -not $fresh.tiers.$tierName.context) {
                $blockers.Add("Pending $tierName context is outside the selection. Restore the original tier configuration before resuming.")
            }
        }
        $policies = $state.PSObject.Properties['conditionalAccessPolicies']
        $keys = @((@($fresh.tiers.privileged.conditionalAccessPolicies) + @($fresh.tiers.lessPrivileged.conditionalAccessPolicies)) | ForEach-Object { $_.key })
        if ($policies) { foreach ($property in $policies.Value.PSObject.Properties) {
            if (($property.Value.PSObject.Properties['pendingCreate'] -or $property.Value.PSObject.Properties['pendingIntent']) -and $property.Name -notin $keys) {
                $blockers.Add("Pending policy $($property.Name) is outside the current controls. Restore the original configuration before resuming.")
            }
        } }
    }
    $digest = Get-PilotDigest $fresh $state $configuration
    if (-not $Apply) {
        $script:pilotReview = $digest
        $receiptPath = Join-Path $pilotDirectory 'azd-pim-gui-receipt.json'
        $receipt = if (Test-Path -LiteralPath $receiptPath) { Get-Content -LiteralPath $receiptPath -Raw | ConvertFrom-Json -Depth 100 } else { $null }
        Write-PilotJson (Join-Path $pilotDirectory 'azd-pim-gui-review.json') @{ identity = @{ account = $binding.account; tenantId = $binding.tenantId; clientId = $binding.clientId }; plan = $fresh; configuration = $Parameters.configuration; receipt = $receipt; approvalRestorable = $false }
        return @{ digest = $digest; plan = $fresh; state = $state; receipt = $receipt; roles = $roles; status = 'reviewRequired'; recoveryBlockers = @($blockers) }
    }
    $reviewed = $script:pilotReview
    $script:pilotReview = $null # Single use even on rejection/failure.
    if (-not $reviewed -or $Parameters.approvedDigest -cne $reviewed -or $digest -cne $reviewed) { throw 'Pilot approval is stale. Recheck and review again.' }
    if ($blockers.Count -gt 0) { throw 'Unresolved writes outside the selected scope block application. Restore the original configuration and recheck.' }
    $attempt = [guid]::NewGuid().ToString()
    $receipt = [ordered]@{ attemptId = $attempt; tenantId = $binding.tenantId; account = $binding.account; digest = $digest; startedAt = [DateTimeOffset]::UtcNow.ToString('o'); status = 'verificationRequired'; completed = $false }
    $receiptPath = Join-Path $pilotDirectory 'azd-pim-gui-receipt.json'
    Write-PilotJson (Join-Path $pilotDirectory "azd-pim-gui-$attempt-review.json") @{
        digest = $digest; plan = $fresh; state = $state; identity = $binding
    }
    Write-PilotJson $receiptPath $receipt
    Write-PilotJson (Join-Path $pilotDirectory "azd-pim-gui-$attempt.json") $receipt
    try {
        $fresh.mode = 'enforced'
        $fresh.configuration.Mode = 'enforced'
        $owned.SetPilotWritesArmed($true)
        $applied = Invoke-AzdPimOwnedApply -Plan $fresh -Context $owned -ExistingState $state -StateChanged {
            param($checkpoint)
            Write-PilotJson (Join-Path $pilotDirectory 'azd-pim-state.json') $checkpoint
        }
        Write-PilotJson (Join-Path $pilotDirectory 'azd-pim-state.json') $applied
        $receipt.status = 'appliedNeedsVerification'
        $receipt.completed = $true
    } catch {
        $receipt.status = 'partialOrUncertain'
        # Detailed engine/HTTP errors can contain private data. Durable state and
        # a fresh plan, rather than exception text, guide reconciliation.
    } finally {
        try { $owned.SetPilotWritesArmed($false) } catch { }
        $receipt.writeResults = @($owned.PilotWriteEvidenceJson() | ConvertFrom-Json)
        $receipt.endedAt = [DateTimeOffset]::UtcNow.ToString('o')
        Write-PilotJson $receiptPath $receipt
        Write-PilotJson (Join-Path $pilotDirectory "azd-pim-gui-$attempt.json") $receipt
    }
    return @{ status = $receipt.status; receipt = $receipt; state = Read-PilotState; digest = $null; plan = $fresh }
}
