#Requires -Version 7.6
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Base,
    [Parameter(Mandatory)][string] $Candidate,
    [Parameter(Mandatory)][string] $Scratch,
    [Parameter(Mandatory)][string] $CargoConfig
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
$target = 'x86_64-pc-windows-msvc'
if (-not $IsWindows -or (Test-Path -LiteralPath $Scratch)) {
    throw 'Compatibility requires Windows and a fresh scratch directory.'
}
$null = New-Item -ItemType Directory -Path $Scratch
$env:CARGO_INCREMENTAL = '0'
$env:CARGO_PROFILE_TEST_DEBUG = '0'

function New-CompatibilityCase($source, $commit, $directory, $rusqlite, $sqliteSys) {
    if ((git -C $source rev-parse HEAD) -ne $commit -or (git -C $source status --porcelain)) {
        throw 'Expected a clean, exact application checkout.'
    }
    $manifest = Join-Path $source 'src-tauri/Cargo.toml'
    $lock = Join-Path $source 'src-tauri/Cargo.lock'
    $app = cargo metadata --manifest-path $manifest --format-version 1 --locked --offline --filter-platform $target |
        ConvertFrom-Json -Depth 100 -AsHashtable
    $src = Join-Path $directory 'src'
    $null = New-Item -ItemType Directory -Path $src
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'SqlitePackageCompatibility.rs') -Destination (Join-Path $src 'lib.rs')
    $persistence = [IO.File]::ReadAllText((Join-Path $source 'src-tauri/src/persistence.rs'))
    $migration = [regex]::Match($persistence, '(?s)const SCHEMA_VERSION: i64 = .*?(?=\r?\nfn recover_interrupted_operations)')
    if (-not $migration.Success -or -not $persistence.Contains('connection.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;')) {
        throw 'Application migration or open settings have drifted.'
    }
    [IO.File]::WriteAllText((Join-Path $src 'app_migration.rs'), $migration.Value)
    $fixtureManifest = Join-Path $directory 'Cargo.toml'
    @"
[package]
name = "azd-gui-sqlite-package-compatibility"
version = "0.0.0"
edition = "2021"

[dependencies]
rusqlite = { version = "=$rusqlite", features = ["bundled"] }
"@ | Set-Content -LiteralPath $fixtureManifest
    Copy-Item -LiteralPath $lock -Destination (Join-Path $directory 'Cargo.lock')
    # Prune the exact application lock for this small synthetic test crate.
    cargo update --manifest-path $fixtureManifest --offline --workspace --quiet
    $fixture = cargo metadata --manifest-path $fixtureManifest --format-version 1 --locked --offline --filter-platform $target |
        ConvertFrom-Json -Depth 100 -AsHashtable
    $appNodes = @{}
    foreach ($node in $app.resolve.nodes) { $appNodes[$node.id] = $node }
    foreach ($node in ($fixture.resolve.nodes | Where-Object { $_.id.StartsWith('registry+') })) {
        $locked = $appNodes[$node.id]
        if (-not $locked -or ($node.deps | Where-Object { $_.pkg -notin $locked.deps.pkg })) {
            throw "Fixture dependency differs from the exact application graph: $($node.id)"
        }
    }
    $prefix = 'registry+https://github.com/rust-lang/crates.io-index#'
    foreach ($package in "rusqlite@$rusqlite", "libsqlite3-sys@$sqliteSys") {
        $id = $prefix + $package
        $node = $fixture.resolve.nodes | Where-Object id -CEQ $id
        if (-not $node -or 'bundled' -notin $node.features -or (Compare-Object $node.features $appNodes[$id].features)) {
            throw "Fixture did not select the locked bundled version and features: $package"
        }
    }
    Write-Host "Verified fixture graph at $commit with rusqlite $rusqlite and libsqlite3-sys $sqliteSys."
    $fixtureManifest
}

function Invoke-CompatibilityTest($manifest, $test) {
    $log = Join-Path (Split-Path -Parent $manifest) "$test.log"
    cargo --config $CargoConfig test --manifest-path $manifest --target $target --locked --offline --lib $test -- --exact --nocapture 2>&1 |
        Tee-Object -FilePath $log
    if (-not (Select-String -LiteralPath $log -SimpleMatch "test $test ... ok") -or
        -not (Select-String -LiteralPath $log -SimpleMatch '1 passed; 0 failed; 0 ignored')) {
        throw "Expected the exact fixture test to pass: $test"
    }
}

$oldDir = Join-Path $Scratch 'old'
$newDir = Join-Path $Scratch 'new'
$old = New-CompatibilityCase $Base '95928e67867c782160c07f1b23e907851162e43c' $oldDir '0.41.1' '0.39.1'
$new = New-CompatibilityCase $Candidate '4794f8ac8a4d0ff25665d2a53ac5924314288063' $newDir '0.43.0' '0.40.0'
Invoke-CompatibilityTest $old 'seed_base'

$original = Join-Path $oldDir 'state.sqlite'
$copy = Join-Path $newDir 'state.sqlite'
if ((Test-Path -LiteralPath "$original-wal") -or (Test-Path -LiteralPath "$original-shm")) {
    throw 'The base fixture is still carrying WAL state.'
}
$originalHash = (Get-FileHash -LiteralPath $original).Hash
Copy-Item -LiteralPath $original -Destination $copy
if ((Get-FileHash -LiteralPath $copy).Hash -ne $originalHash) {
    throw 'The candidate did not receive an exact copy of the checkpointed fixture.'
}
Invoke-CompatibilityTest $new 'upgrade_copy'
if ((Get-FileHash -LiteralPath $original).Hash -ne $originalHash) {
    throw 'The original fixture changed during candidate testing.'
}
Write-Host "Checkpointed base fixture preserved (SHA-256 $originalHash)."
