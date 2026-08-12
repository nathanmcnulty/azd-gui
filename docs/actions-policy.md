# GitHub Actions usage policy

[GitHub-hosted runner usage is metered for private repositories](https://docs.github.com/en/billing/concepts/product-billing/github-actions), and Windows and macOS runners cost more per minute than Linux. Hosted runners are therefore a release resource for this repository, not the default development environment. Ordinary pushes, pull requests, Dependabot updates, documentation edits, and merges must consume zero Actions minutes.

## Development validation

Run the complete validation suite locally before requesting review:

```powershell
npm ci
npm run verify
npm run audit:dependencies
```

`npm run verify` performs Svelte and TypeScript checks, frontend tests, the production frontend build, Rust formatting, Clippy with warnings denied, and Rust tests.

The `Manual validation` workflow has only a `workflow_dispatch` trigger. A dispatch validates exactly one selected runner: Windows, Linux, or macOS. Use it only when a platform-specific result cannot reasonably be established on a local machine. Cross-platform validation requires three separate, visible decisions rather than an automatically expanded matrix.

Manual validation runs are capped at 45 minutes and duplicate runs for the same ref and platform cancel the older run.

## Releases

Creating a `v*` tag is the only automatic Actions trigger in the repository. It starts one Windows job that:

1. Audits the npm and Cargo lockfiles for known vulnerabilities using pinned `cargo-audit` 0.22.2.
2. Runs frontend checks and tests.
3. Runs Rust formatting, Clippy, and tests.
4. Builds the portable executable.
5. Authenticates to Azure using GitHub OIDC after the build succeeds.
6. Signs the executable, creates and signs the NSIS installer, and verifies both signatures.
7. Generates SHA-256 checksums and uploads the files as a 14-day workflow artifact.
8. For an actual `v*` tag, publishes the same verified files to a durable GitHub Release.

The release workflow can be dispatched manually only after selecting its explicit cost and signing confirmation. It is capped at 45 minutes, and a duplicate run for the same ref cancels the older run.

macOS and Linux packaging remain local/manual until those release artifacts are actively supported. Do not add them to the tagged release workflow merely to prove that compilation is possible.

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
