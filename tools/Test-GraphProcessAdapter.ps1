param([Parameter(Mandatory)][string]$BuildRoot)
$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$managed = Join-Path $BuildRoot 'managed-offline-adapter'
dotnet --version
pwsh --version
cargo --version
dotnet publish (Join-Path $repository 'src-tauri/graph-adapter-tests/AzdGui.GraphAdapter.Tests.csproj') --configuration Release --output $managed -p:RestoreLockedMode=true --nologo
if ($LASTEXITCODE -ne 0) { throw 'Managed offline Graph adapter build failed.' }
$env:AZD_GUI_GRAPH_TEST_ADAPTER_DIR = $managed
cargo test --manifest-path (Join-Path $repository 'src-tauri/Cargo.toml') graph_session::tests -- --include-ignored --nocapture
if ($LASTEXITCODE -ne 0) { throw 'Offline real-process Graph adapter tests failed.' }
