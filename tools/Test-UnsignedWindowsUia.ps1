#Requires -Version 7.6
[CmdletBinding()]
param([Parameter(Mandatory)][string] $Source)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$sourceHead = '9ac2cd0a54bf0048d52a4da663062f40e4df1801'
$identifier = 'dev.azd-gui.tauri218-uia'

# The NSIS install and roaming fixture belong to the disposable job VM.
if (-not $IsWindows -or $env:GITHUB_ACTIONS -cne 'true' -or
    $env:RUNNER_ENVIRONMENT -cne 'github-hosted' -or
    $env:GITHUB_REPOSITORY -cne 'nathanmcnulty/azd-gui' -or
    $env:GITHUB_EVENT_NAME -cne 'pull_request' -or -not $env:RUNNER_TEMP) {
    throw 'This smoke may install only in a disposable GitHub-hosted Windows pull-request job.'
}
$sourceRoot = (Resolve-Path -LiteralPath $Source).ProviderPath
if ((git -C $sourceRoot rev-parse HEAD) -cne $sourceHead -or (git -C $sourceRoot status --porcelain)) {
    throw 'Unsigned smoke requires the exact clean source.'
}
$scratch = Join-Path $env:RUNNER_TEMP 'azd-gui-unsigned218-uia'
$profile = Join-Path ([Environment]::GetFolderPath('ApplicationData')) $identifier
if ((Test-Path -LiteralPath $scratch) -or (Test-Path -LiteralPath $profile)) {
    throw 'The isolated install or profile is already in use.'
}
if (-not [Environment]::UserInteractive -or (Get-Process -Id $PID).SessionId -eq 0) {
    throw 'The standard runner must supply an interactive desktop before building.'
}
if ((Get-Item -LiteralPath $env:RUNNER_TEMP).PSDrive.Free -lt 4GB) { throw 'Unsigned build needs 4 GiB of scratch.' }

$null = New-Item -ItemType Directory -Path $scratch, $profile
function Get-WebView2Version {
    $client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    @(
        (Get-ItemPropertyValue -LiteralPath "HKLM:/SOFTWARE/WOW6432Node/Microsoft/EdgeUpdate/Clients/$client" -Name pv -ErrorAction SilentlyContinue)
        (Get-ItemPropertyValue -LiteralPath "HKCU:/Software/Microsoft/EdgeUpdate/Clients/$client" -Name pv -ErrorAction SilentlyContinue)
    ) | Where-Object { $_ -and ([version]$_ -gt [version]'0.0.0.0') } | Select-Object -First 1
}
$runtime = Get-WebView2Version
if (-not $runtime) {
    $bootstrapper = Join-Path $scratch 'MicrosoftEdgeWebview2Setup.exe'
    Invoke-WebRequest -Uri 'https://go.microsoft.com/fwlink/p/?LinkId=2124703' -OutFile $bootstrapper
    $signature = Get-AuthenticodeSignature -LiteralPath $bootstrapper
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '^CN=Microsoft Corporation,') {
        throw 'WebView2 bootstrapper requires a valid Microsoft signature.'
    }
    $setup = Start-Process -FilePath $bootstrapper -ArgumentList '/silent', '/install' -WindowStyle Hidden -Wait -PassThru
    $runtime = Get-WebView2Version
    if ($setup.ExitCode -ne 0 -or -not $runtime) { throw 'Microsoft WebView2 runtime installation failed.' }
}

$install = Join-Path $scratch 'installed'
$project = Join-Path $scratch 'fixture'
$env:CARGO_TARGET_DIR = Join-Path $scratch 'cargo-target'
$overlay = Join-Path $scratch 'tauri-smoke.json'
[ordered]@{
    identifier = $identifier
    productName = 'AZD GUI Tauri 218 UIA'
    bundle = @{ windows = @{ nsis = @{ installMode = 'currentUser' } } }
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $overlay -Encoding utf8NoBOM

$driverRoot = Join-Path $PSScriptRoot 'UnsignedWindowsUia'
$driverOut = Join-Path $scratch 'driver'
$dotnet = (Get-Command dotnet -CommandType Application).Source
& $dotnet build (Join-Path $driverRoot 'Driver.csproj') --nologo -o $driverOut -p:BaseIntermediateOutputPath="$scratch/driver-obj/"
$driver = Join-Path $driverOut 'Driver.dll'
& $dotnet $driver --check-desktop

Write-Output "Source $sourceHead, WebView2 $runtime"
Push-Location -LiteralPath $sourceRoot
try {
    & (Join-Path $sourceRoot 'scripts/assert-powershell-version.ps1')
    node --version
    npm --version
    rustc --version
    dotnet --version
    npm ci --no-audit --no-fund
    npm run tauri -- build --no-bundle --ci --config $overlay -- --locked
    npm run tauri -- bundle --bundles nsis --ci --config $overlay
} finally {
    Pop-Location
}

$release = Join-Path $env:CARGO_TARGET_DIR 'release'
$installers = @(Get-ChildItem -LiteralPath (Join-Path $release 'bundle/nsis') -File -Filter '*_x64-setup.exe')
if ($installers.Count -ne 1) { throw 'Expected one x64 NSIS installer.' }
$nsis = $installers[0].FullName
if ((Get-AuthenticodeSignature -LiteralPath $nsis).Status -ne 'NotSigned') { throw 'Expected an unsigned NSIS installer.' }
$setup = Start-Process -FilePath $nsis -ArgumentList @('/S', ('/D=' + $install)) -WindowStyle Hidden -Wait -PassThru
if ($setup.ExitCode -ne 0) { throw "NSIS installer exited $($setup.ExitCode)." }
$exe = Join-Path $install 'azd-gui.exe'
$exeHash = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash
if ((Get-AuthenticodeSignature -LiteralPath $exe).Status -ne 'NotSigned') { throw 'Expected an unsigned installed executable.' }
Write-Output "Unsigned installer SHA-256 $((Get-FileHash -LiteralPath $nsis -Algorithm SHA256).Hash)"
Write-Output "Installed executable SHA-256 $exeHash"
$resources = @('catalog/azd-gui.catalog.json') +
    @(Get-ChildItem -LiteralPath (Join-Path $sourceRoot 'catalog/manifests') -File -Filter '*.json' | ForEach-Object { 'catalog/manifests/' + $_.Name }) +
    @(Get-ChildItem -LiteralPath (Join-Path $sourceRoot 'schemas') -File -Filter '*.json' | ForEach-Object { 'schemas/' + $_.Name })
foreach ($resource in $resources) {
    $original = (Get-FileHash -LiteralPath (Join-Path $sourceRoot $resource)).Hash
    $shipped = (Get-FileHash -LiteralPath (Join-Path $install "_up_/$resource")).Hash
    if ($original -ne $shipped) { throw "Installed resource differs from source: $resource" }
}
Write-Output "Verified $($resources.Count) installed resources"

node (Join-Path $driverRoot 'seed.cjs') $project (Join-Path $profile 'azd-gui.sqlite')
$fixture = Join-Path $project 'component.txt'
$lock = Join-Path $project 'azd-components.lock.json'
$solutionFile = Join-Path $project 'solution-owned.txt'
$fixtureBefore = (Get-FileHash -LiteralPath $fixture).Hash
$lockBefore = (Get-FileHash -LiteralPath $lock).Hash
$solutionBefore = (Get-FileHash -LiteralPath $solutionFile).Hash

# The pinned Rust update feed bypasses process proxies. Restrict just its
# known host to loopback during this disposable job and block app egress.
$feed = 'api.github.com'
$hostsFile = Join-Path $env:SystemRoot 'System32/drivers/etc/hosts'
if ((Get-Item -LiteralPath $hostsFile -Force).Attributes.HasFlag([IO.FileAttributes]::ReparsePoint)) {
    throw 'The runner hosts file must be a regular file.'
}
$hostsOriginal = [IO.File]::ReadAllBytes($hostsFile)
if ([Text.Encoding]::ASCII.GetString($hostsOriginal) -match '(?im)^[^#\r\n]+[ \t]api\.github\.com(?:\s|$)') {
    throw 'The runner already has an update host override.'
}
$lineEnding = [string][char]13 + [char]10
$hostsTest = [byte[]]($hostsOriginal + [Text.Encoding]::ASCII.GetBytes(
    ( @('', "127.0.0.1 $feed", "::1 $feed", '') -join $lineEnding )
))
$hostsTestHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($hostsTest))
$firewallName = 'azd-gui-unsigned-uia-' + [guid]::NewGuid()
$hostsChanged = $false
$ruleCreated = $false
$testTemp = Join-Path $scratch 'runtime'
$emptyPath = Join-Path $scratch 'no-tools'
$null = New-Item -ItemType Directory -Path $testTemp, $emptyPath

function New-SmokeProcessInfo([string] $Executable) {
    $info = [Diagnostics.ProcessStartInfo]::new($Executable)
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.Environment.Clear()
    foreach ($key in 'SystemRoot', 'SystemDrive', 'USERPROFILE', 'APPDATA', 'LOCALAPPDATA') {
        $info.Environment[$key] = [Environment]::GetEnvironmentVariable($key)
    }
    $info.Environment['PATH'] = $emptyPath
    $info.Environment['TEMP'] = $testTemp
    $info.Environment['TMP'] = $testTemp
    return $info
}

$app = $null
$uia = $null
try {
    if (Get-NetFirewallProfile -PolicyStore ActiveStore | Where-Object Enabled -ne 'True') {
        throw 'The runner does not enforce Windows Firewall on every profile.'
    }
    $null = New-NetFirewallRule -Name $firewallName -DisplayName $firewallName -Direction Outbound -Action Block -Program $exe -Profile Any -Enabled True
    $ruleCreated = $true
    $active = Get-NetFirewallRule -Name $firewallName -PolicyStore ActiveStore
    if ($active.Enabled -ne 'True' -or $active.Direction -ne 'Outbound' -or
        $active.Action -ne 'Block' -or ($active | Get-NetFirewallApplicationFilter).Program -ne $exe) {
        throw 'The exact installed process egress block is unavailable.'
    }

    [IO.File]::WriteAllBytes($hostsFile, $hostsTest)
    $hostsChanged = $true
    $answers = @(Resolve-DnsName -Name $feed -CacheOnly -ErrorAction SilentlyContinue | Where-Object IPAddress)
    if ($answers.Count -eq 0 -or ($answers | Where-Object {-not [Net.IPAddress]::IsLoopback([ipaddress]$_.IPAddress)})) {
        throw 'The Windows resolver did not verify the test feed as loopback only.'
    }
    if (Get-NetTCPConnection -State Listen -LocalPort 1,443 -ErrorAction SilentlyContinue) {
        throw 'A local listener would invalidate the smoke egress boundary.'
    }

    $appInfo = New-SmokeProcessInfo $exe
    $appInfo.WorkingDirectory = $install
    $appInfo.Environment['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $scratch 'webview'
    $appInfo.Environment['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = '--disable-background-networking --disable-quic --proxy-server=http://127.0.0.1:1 --proxy-bypass-list=<-loopback>;tauri.localhost;ipc.localhost'
    $app = [Diagnostics.Process]::Start($appInfo)
    $uiaInfo = New-SmokeProcessInfo $dotnet
    foreach ($arg in $driver, '--inspect', $app.Id, 'Azure Deployment Studio', $fixture) {
        $uiaInfo.ArgumentList.Add([string]$arg)
    }
    $uia = [Diagnostics.Process]::Start($uiaInfo)
    if (-not $uia.WaitForExit(90000)) { throw 'The UI Automation driver did not finish within 90 seconds.' }
    if ($uia.ExitCode -ne 0) { throw 'The installed component UI Automation check failed.' }
    if ((Get-FileHash -LiteralPath $fixture).Hash -eq $fixtureBefore -or
        (Get-FileHash -LiteralPath $lock).Hash -ne $lockBefore -or
        (Get-FileHash -LiteralPath $solutionFile).Hash -ne $solutionBefore) {
        throw 'The synthetic drift or preserved fixture files did not match the test.'
    }
    Write-Output 'Exact installed app showed Verified then Modified through native component provenance.'
} finally {
    try {
        if ($uia -and -not $uia.HasExited) {
            $uia.Kill()
            if (-not $uia.WaitForExit(5000)) { throw 'Owned UI Automation driver did not stop.' }
        }
        if ($app -and -not $app.HasExited) {
            if (-not $app.CloseMainWindow() -or -not $app.WaitForExit(5000)) {
                $app.Kill($true)
                if (-not $app.WaitForExit(5000)) { throw 'Owned installed app did not stop.' }
            }
        }
    } finally {
        try {
            if ($hostsChanged) {
                if ((Get-FileHash -LiteralPath $hostsFile).Hash -ne $hostsTestHash) { throw 'The runner hosts file changed during the smoke.' }
                [IO.File]::WriteAllBytes($hostsFile, $hostsOriginal)
                $expected = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($hostsOriginal))
                if ((Get-FileHash -LiteralPath $hostsFile).Hash -ne $expected) { throw 'The prior runner hosts file was not restored.' }
            }
        } finally {
            if ($ruleCreated) { Remove-NetFirewallRule -Name $firewallName }
        }
    }
}
