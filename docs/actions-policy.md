# GitHub Actions usage policy

[GitHub-hosted runner usage is metered for private repositories](https://docs.github.com/en/billing/concepts/product-billing/github-actions), and Windows and macOS runners cost more per minute than Linux. Hosted runners are therefore a release resource for this repository, not the default development environment. Ordinary pushes, pull requests, Dependabot updates, documentation edits, and merges must consume zero Actions minutes.

## Development validation

Run the complete validation suite locally before requesting review. Maintained
PowerShell tooling uses PowerShell 7.6+ and the stable Pester 6.2.0 release. Install the
test module explicitly (the runner never installs it):

```powershell
Install-Module Pester -RequiredVersion 6.2.0 -Scope CurrentUser -Repository PSGallery
npm ci
npm run verify
npm run audit:dependencies
```

`npm run verify` performs Svelte and TypeScript checks, frontend tests, the production frontend build, the offline signing/verifier Pester suites, Rust formatting, Clippy with warnings denied, and Rust tests.

`npm run test:powershell` is the focused offline entry point. It reports engine/module
versions and actual test counts and rejects empty, skipped, failed, or incomplete
runs. Signing and signature inspection are mocked, including fail-closed test-only
command shims on non-Windows hosts; these tests do not establish real Authenticode
or packaging acceptance. Both workflows assert PowerShell 7.6+ and install exactly
Pester 6.2.0 before running this check. The signed release does so before Azure
sign-in and all signing operations. An older hosted engine fails explicitly.

The `Manual validation` workflow has only a `workflow_dispatch` trigger. A dispatch validates exactly one selected runner: Windows, Linux, or macOS. Use it only when a platform-specific result cannot reasonably be established on a local machine. Cross-platform validation requires three separate, visible decisions rather than an automatically expanded matrix.

Manual validation runs are capped at 45 minutes and duplicate runs for the same ref and platform cancel the older run.

## Releases

Creating a `v*` tag is the only automatic Actions trigger in the repository. For a version tag in this repository, the Windows job verifies the protected annotated tag, the pinned SSH signer, and the exact commit on main before proceeding. The Windows job then:

1. Audits the npm and Cargo lockfiles for known vulnerabilities using pinned `cargo-audit` 0.22.2.
2. Runs frontend checks and tests.
3. Runs Rust formatting, Clippy, and tests.
4. Builds the portable executable.
5. Authenticates to Azure using GitHub OIDC after the build succeeds.
6. Uses Tauri's signing hook with pinned ArtifactSigning 0.1.8 to sign the patched NSIS payload, uninstaller, and installer, then signs the restored portable executable. Signing uses only the OIDC-authenticated Azure CLI session.
7. Generates SHA-256 checksums, installs the package in a disposable runner directory, verifies the installed executable's signature, signer, timestamp, and exact version, and uninstalls it before uploading the files and verification evidence as a 14-day workflow artifact. This packaging check does not replace clean-VM standard-user or visual acceptance.
8. For a version tag, adds the exact source bundle to that run's workflow artifact.

For a version tag, an Ubuntu job validates the immutable artifact from the
Windows job and attests its source bundle, signed portable executable, and
signed NSIS installer. A separate Ubuntu job stages the existing release
assets as a draft only after successful attestation. It validates the same
artifact and refuses an already public release. Only the attestation job has
attestation write access. Rotating the release-tag signing key requires a
reviewed update to the public key in scripts/release-tag-allowed-signers.

The release workflow can be dispatched manually only after selecting its explicit cost and signing confirmation. The Windows job is capped at 45 minutes, and each tag-only Ubuntu job is capped at 15 minutes. A duplicate run for the same ref cancels the older run.

The draft is a publication boundary, not a finished release. A maintainer must complete [Windows beta acceptance](windows-beta-acceptance.md), retain its evidence, and explicitly publish the draft. Failed or incomplete acceptance leaves the release private.

The planned beta release target is Windows 11 on x64. Windows 10, macOS, and Linux are outside beta acceptance. macOS and Linux packaging remain experimental and local/manual until those release artifacts have dedicated packaging, signing, and acceptance gates. Do not add them to the tagged release workflow merely to prove that compilation is possible.

## Repository settings

Because pull requests intentionally have no automatic checks, branch protection must not require Actions status contexts. Keep the remaining protections enabled:

- pull requests required;
- stale approvals dismissed;
- administrators included;
- linear history required;
- force pushes and branch deletion disabled;
- review conversations resolved before merge.

Enable GitHub included-usage alerts and an Actions budget alert under **Settings → Billing & plans → Budgets and alerts**. GitHub documents the available controls in [Budgets and alerts](https://docs.github.com/en/billing/concepts/budgets-and-alerts). A budget is a final guardrail, not permission to add automatic development workflows.

## Change control

Any proposal that adds `push`, `pull_request`, `schedule`, or automatic matrix triggers must include:

- the concrete release requirement that cannot be met locally;
- the expected runner platforms and typical billed duration;
- the expected number of runs per month;
- a less expensive alternative and why it is insufficient;
- explicit maintainer approval before merge.

Action references remain pinned to immutable commit hashes. Dependency update pull requests may propose newer hashes, but they do not receive automatic hosted validation.
