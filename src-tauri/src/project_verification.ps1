$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
# Only embedded validation code is loaded. No project script is imported.
& (Join-Path $PSScriptRoot 'scripts/Test-Deployment.ps1')
