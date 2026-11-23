#Requires -Version 7.6
[CmdletBinding()]
param(
    [Parameter(Mandatory, Position = 0)][string] $TestExecutable,
    [Parameter(ValueFromRemainingArguments)][string[]] $TestArguments
)

$ErrorActionPreference = 'Stop'
$target = (Resolve-Path -LiteralPath $env:CARGO_TARGET_DIR).ProviderPath
$program = (Resolve-Path -LiteralPath $TestExecutable).ProviderPath
if (-not $IsWindows -or -not $program.StartsWith($target.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -or
    [IO.Path]::GetExtension($program) -ne '.exe') {
    throw 'The isolated runner accepts only a Windows executable in the native target.'
}

$profile = Join-Path $env:TEMP ('azd-gui-native-test-' + [guid]::NewGuid())
$temp = Join-Path $profile 'temp'
$roaming = Join-Path $profile 'roaming'
$local = Join-Path $profile 'local'
$null = New-Item -ItemType Directory -Path $profile, $temp, $roaming, $local
$gitConfig = Join-Path $profile 'gitconfig'
$null = New-Item -ItemType File -Path $gitConfig
$gitDir = Split-Path -Parent (Get-Command git -CommandType Application | Select-Object -First 1).Source
$pwshDir = Split-Path -Parent (Get-Process -Id $PID).Path
$system = Join-Path $env:SystemRoot 'System32'

$info = [Diagnostics.ProcessStartInfo]::new($program)
$info.WorkingDirectory = (Get-Location).ProviderPath
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.Environment.Clear()
$info.Environment['SystemRoot'] = $env:SystemRoot
$info.Environment['SystemDrive'] = $env:SystemDrive
$info.Environment['PATH'] = @($system, $gitDir, $pwshDir) -join ';'
$info.Environment['TEMP'] = $temp
$info.Environment['TMP'] = $temp
$info.Environment['USERPROFILE'] = $profile
$info.Environment['APPDATA'] = $roaming
$info.Environment['LOCALAPPDATA'] = $local
$info.Environment['AZURE_CONFIG_DIR'] = Join-Path $profile 'azure'
$info.Environment['AZD_CONFIG_DIR'] = Join-Path $profile 'azd'
$info.Environment['AZURE_DEV_CONFIG_DIR'] = $info.Environment['AZD_CONFIG_DIR']
$info.Environment['GH_CONFIG_DIR'] = Join-Path $profile 'gh'
$info.Environment['GIT_CONFIG_NOSYSTEM'] = '1'
$info.Environment['GIT_CONFIG_GLOBAL'] = $gitConfig
$info.Environment['GIT_TERMINAL_PROMPT'] = '0'
$info.Environment['GIT_ALLOW_PROTOCOL'] = 'file'
foreach ($arg in $TestArguments) { $info.ArgumentList.Add($arg) }

$process = [Diagnostics.Process]::Start($info)
try {
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(300000)) { throw 'The owned native test executable timed out.' }
    [Console]::Out.Write($stdout.GetAwaiter().GetResult())
    [Console]::Error.Write($stderr.GetAwaiter().GetResult())
    exit $process.ExitCode
} finally {
    if (-not $process.HasExited) {
        $process.Kill($true)
        $process.WaitForExit(5000) | Out-Null
    }
    $process.Dispose()
}
