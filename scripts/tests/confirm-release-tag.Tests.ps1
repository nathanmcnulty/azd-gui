#Requires -Version 7.6

BeforeAll {
    $guard = Join-Path $PSScriptRoot '../confirm-release-tag.ps1'
    function gh {
        $global:LASTEXITCODE = 0
        if ($args[-1] -ceq 'repos/nathanmcnulty/azd-gui/git/ref/tags/v0.1.1') {
            $mockTag.Ref | ConvertTo-Json -Depth 5
        } elseif ($args[-1] -ceq "repos/nathanmcnulty/azd-gui/git/tags/$($mockTag.Oid)") {
            $mockTag.Object | ConvertTo-Json -Depth 5
        } else {
            throw 'Unexpected offline GitHub route.'
        }
    }
    function git {
        $global:LASTEXITCODE = 0
        if ($args -contains 'rev-parse') {
            if ($args[-1] -ceq 'HEAD^{commit}') { $mockTag.Head }
            elseif ($args[-1] -ceq 'refs/tags/v0.1.1^{tag}') { $mockTag.Local }
            else { throw 'Unexpected offline Git ref.' }
        } elseif ($args -contains 'merge-base') {
            $args[-1] | Should -BeExactly 'refs/remotes/origin/main'
            $global:LASTEXITCODE = $mockTag.AncestryExit
        } elseif ($args -contains 'verify-tag') {
            $args[-1] | Should -BeExactly $mockTag.Oid
            $args | Should -Contain 'gpg.format=ssh'
            $args | Where-Object { $_ -like 'gpg.ssh.allowedSignersFile=*release-tag-allowed-signers' } |
                Should -Not -BeNullOrEmpty
            $global:LASTEXITCODE = $mockTag.SignatureExit
        } else {
            throw 'Unexpected offline Git command.'
        }
    }
}

Describe 'Signed release tag gate' {
    BeforeEach {
        $commit = '1' * 40
        $oid = '2' * 40
        $mockTag = @{
            Oid = $oid
            Ref = @{ ref = 'refs/tags/v0.1.1'; object = @{ type = 'tag'; sha = $oid } }
            Object = @{
                sha = $oid; tag = 'v0.1.1'; object = @{ type = 'commit'; sha = $commit }
                verification = @{ verified = $true }
            }
            Head = $commit
            Local = $oid
            AncestryExit = 0
            SignatureExit = 0
        }
        $release = @{ ReleaseRef = 'refs/tags/v0.1.1'; EventCommit = $commit }
    }

    It 'requires the same signed object in the remote, checkout and later jobs' {
        & $guard @release | Should -BeExactly $oid
        & $guard @release -ExpectedTagObject $oid | Should -BeExactly $oid
        { & $guard @release -ExpectedTagObject ('3' * 40) } | Should -Throw '*changed after Windows*'
    }

    It 'rejects a lightweight tag' {
        $mockTag.Ref.object.type = 'commit'
        { & $guard @release } | Should -Throw '*exact annotated tag*'
    }

    It 'requires GitHub verification for the exact commit' {
        $mockTag.Object.verification.verified = $false
        { & $guard @release } | Should -Throw '*GitHub did not verify*'
        $mockTag.Object.verification.verified = $true
        $mockTag.Object.object.sha = '3' * 40
        { & $guard @release } | Should -Throw '*exact event commit*'
    }

    It 'rejects an otherwise verified tag with a different signer' {
        $mockTag.SignatureExit = 1
        { & $guard @release } | Should -Throw '*pinned signer*'
    }

    It 'rejects an unreviewed source commit and checkout drift' {
        $mockTag.AncestryExit = 1
        { & $guard @release } | Should -Throw '*must be on main*'
        $mockTag.AncestryExit = 0
        $mockTag.Local = '3' * 40
        { & $guard @release } | Should -Throw '*checked-out tag object differs*'
        $mockTag.Local = $oid
        $mockTag.Head = '3' * 40
        { & $guard @release } | Should -Throw '*HEAD differs*'
    }
}
