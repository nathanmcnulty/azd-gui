param([Parameter(Mandatory)][string]$BuildRoot)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$repository = Split-Path -Parent $PSScriptRoot
$managed = Join-Path $BuildRoot 'managed-offline-adapter'
dotnet --version
pwsh --version
cargo --version
dotnet publish (Join-Path $repository 'src-tauri/graph-adapter-tests/AzdGui.GraphAdapter.Tests.csproj') --configuration Release --output $managed -p:RestoreLockedMode=true --nologo
if ($LASTEXITCODE -ne 0) { throw 'Managed offline pilot adapter build failed.' }
$previous = $env:AZD_GUI_GRAPH_TEST_ADAPTER_DIR
try {
    $env:AZD_GUI_GRAPH_TEST_ADAPTER_DIR = $managed
    # The adapter replaces only MSAL/HTTP. The native process boundary, pinned
    # PowerShell planner/apply engine and durable checkpoint files remain real.
    cargo test --manifest-path (Join-Path $repository 'src-tauri/Cargo.toml') pilot_worker_ -- --include-ignored --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw 'Offline real-worker pilot tests failed.' }
} finally {
    $env:AZD_GUI_GRAPH_TEST_ADAPTER_DIR = $previous
}
