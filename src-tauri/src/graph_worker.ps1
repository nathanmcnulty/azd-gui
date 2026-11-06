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
                $owned.Probe()
            }
            elseif ($request.action -eq 'dispose') { break }
            else { throw 'Unsupported Graph operation.' }
            $evidence = if ($connected) { $owned.EvidenceJson() | ConvertFrom-Json -Depth 12 } else { $null }
            $reply = [ordered]@{ sequence=$sequence; sessionId=$session; requirementsDigest=$binding.requirementsDigest; processId=$PID; action=[string]$request.action; ok=$true; evidence=$evidence }
            [Console]::Out.WriteLine('AZD_GRAPH_RPC:' + ($reply | ConvertTo-Json -Depth 12 -Compress))
            [Console]::Out.Flush()
        }
        catch {
            # Never serialize raw SDK errors, AuthenticationResult or HTTP data.
            $reply = [ordered]@{ sequence=$sequence; sessionId=$session; requirementsDigest=if($binding){$binding.requirementsDigest}else{''}; processId=$PID; action=if($request){[string]$request.action}else{''}; ok=$false; evidence=$null }
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
