param([Parameter(Mandatory)][string]$BuildRoot)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
node --version
cargo --version
pwsh --version
npm --prefix $repository run ci
if ($LASTEXITCODE -ne 0) { throw 'Frontend journey checks failed.' }
cargo test --manifest-path (Join-Path $repository 'src-tauri/Cargo.toml') validation_worker_ -- --include-ignored --nocapture --test-threads=1
if ($LASTEXITCODE -ne 0) { throw 'Deployment-check real validation fixture failed.' }
& (Join-Path $PSScriptRoot 'Test-PimPilot.ps1') -BuildRoot $BuildRoot
