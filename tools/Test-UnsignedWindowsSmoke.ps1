#Requires -Version 7.6
[CmdletBinding()]
param([Parameter(Mandatory)][string] $Source)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$sourceHead = 'c8d7df8c7d7262bf5078a368704fc762f4bc4c1f'
$identifier = 'dev.azd-gui.npm154-acceptance'
$port = 19454

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
$scratch = Join-Path $env:RUNNER_TEMP 'azd-gui-unsigned154'
$profile = Join-Path ([Environment]::GetFolderPath('ApplicationData')) $identifier
if ((Test-Path -LiteralPath $scratch) -or (Test-Path -LiteralPath $profile) -or
    (Get-NetTCPConnection -State Listen -LocalPort $port -ErrorAction SilentlyContinue)) {
    throw 'The isolated install, profile or debugger port is already in use.'
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
    productName = 'AZD GUI npm 154 Acceptance'
    bundle = @{ windows = @{ nsis = @{ installMode = 'currentUser' } } }
} | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $overlay -Encoding utf8NoBOM

$harness = Join-Path $PSScriptRoot 'unsigned-windows-smoke.cjs'
node $harness --self-test
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

node $harness --seed $project (Join-Path $profile 'azd-gui.sqlite')
$env:NO_PROXY = '127.0.0.1,localhost'
node $harness --probe $project $port $exe $scratch
Write-Output 'Installed app and its debugger stopped. The disposable job VM owns the install and profile cleanup.'
