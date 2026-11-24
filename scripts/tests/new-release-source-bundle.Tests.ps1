BeforeAll {
    $archiveScript = Join-Path $PSScriptRoot '../new-release-source-bundle.ps1'
    $scratch = (Get-Item -LiteralPath TestDrive:\).FullName
    $repository = (New-Item -ItemType Directory -Path (Join-Path $scratch 'source repository')).FullName

    & git -C $repository init --quiet --template=
    if ($LASTEXITCODE -ne 0) { throw 'Could not initialize the synthetic repository.' }

    $file = Join-Path $repository 'fixture.txt'
    Set-Content -LiteralPath $file -Value 'first committed content' -NoNewline
    & git -C $repository add -- fixture.txt
    if ($LASTEXITCODE -ne 0) { throw 'Could not stage the first fixture.' }
    & git -C $repository -c "core.hooksPath=$scratch" -c user.name=Fixture -c user.email=fixture@example.invalid commit --quiet --no-gpg-sign -m first
    if ($LASTEXITCODE -ne 0) { throw 'Could not commit the first fixture.' }
    $firstCommit = & git -C $repository rev-parse HEAD
    $tree = & git -C $repository rev-parse 'HEAD^{tree}'

    Set-Content -LiteralPath $file -Value 'second committed content' -NoNewline
    & git -C $repository -c "core.hooksPath=$scratch" -c user.name=Fixture -c user.email=fixture@example.invalid commit --quiet --no-gpg-sign -am second
    if ($LASTEXITCODE -ne 0) { throw 'Could not commit the second fixture.' }
    Set-Content -LiteralPath $file -Value 'local changed content' -NoNewline
    Set-Content -LiteralPath (Join-Path $repository 'untracked.txt') -Value 'local untracked content' -NoNewline
}

AfterAll {
    # Git creates read-only object files. Let Pester remove its TestDrive on Windows.
    $objects = Join-Path $repository '.git/objects'
    if (Test-Path -LiteralPath $objects) {
        Get-ChildItem -LiteralPath $objects -Recurse -File -Force | Where-Object IsReadOnly | ForEach-Object {
            $_.IsReadOnly = $false
        }
    }
}

Describe 'Local release source bundle' {
    BeforeEach {
        $output = (New-Item -ItemType Directory -Path (Join-Path $scratch ([guid]::NewGuid().ToString('N')))).FullName
    }

    It 'archives the requested commit with reproducible contents and digest' {
        $secondOutput = (New-Item -ItemType Directory -Path (Join-Path $output 'repeat archive')).FullName
        $archive = & $archiveScript -RepositoryRoot $repository -Commit $firstCommit -OutputDirectory $output
        $repeat = & $archiveScript -RepositoryRoot $repository -Commit $firstCommit -OutputDirectory $secondOutput

        $archive.Commit | Should -BeExactly $firstCommit
        $archive.Sha256 | Should -BeExactly (Get-FileHash -LiteralPath $archive.Path -Algorithm SHA256).Hash.ToLowerInvariant()
        $repeat.Sha256 | Should -BeExactly $archive.Sha256
        (Split-Path $archive.Path -Leaf) | Should -BeExactly "azd-gui-$firstCommit.tar.gz"

        $prefix = "azd-gui-$firstCommit"
        $entries = @(& tar -tf $archive.Path)
        $LASTEXITCODE | Should -Be 0
        $entries | Should -Contain "$prefix/fixture.txt"
        $entries | Should -Not -Contain "$prefix/untracked.txt"
        $content = & tar -xOf $archive.Path "$prefix/fixture.txt"
        $LASTEXITCODE | Should -Be 0
        $content | Should -BeExactly 'first committed content'
    }

    It 'requires the complete commit ID' {
        { & $archiveScript -RepositoryRoot $repository -Commit HEAD -OutputDirectory $output } | Should -Throw
        @(Get-ChildItem -LiteralPath $output -Force).Count | Should -Be 0
    }

    It 'rejects an exact non-commit object ID' {
        { & $archiveScript -RepositoryRoot $repository -Commit $tree -OutputDirectory $output } |
            Should -Throw '*local Git commit object*'
        @(Get-ChildItem -LiteralPath $output -Force).Count | Should -Be 0
    }

    It 'fails locally when the exact object is unavailable' {
        { & $archiveScript -RepositoryRoot $repository -Commit ('0' * 40) -OutputDirectory $output } |
            Should -Throw '*local Git commit object*'
        @(Get-ChildItem -LiteralPath $output -Force).Count | Should -Be 0
    }

    It 'preserves an existing archive' {
        $path = Join-Path $output "azd-gui-$firstCommit.tar.gz"
        Set-Content -LiteralPath $path -Value 'keep existing bytes' -NoNewline

        { & $archiveScript -RepositoryRoot $repository -Commit $firstCommit -OutputDirectory $output } |
            Should -Throw '*already exists*'

        Get-Content -LiteralPath $path -Raw | Should -BeExactly 'keep existing bytes'
        @(Get-ChildItem -LiteralPath $output -Force).Count | Should -Be 1
    }
}
