# Windows readiness and template adoption

Implementation follow-up to the 12 September 2026 review. This branch establishes a local development baseline; signed Windows acceptance and a real deployment pilot remain separate completion gates.

## Local validation

On Windows with Node.js 22.23.2, `npm run verify` passed: zero Svelte/TypeScript errors or warnings, 56 frontend tests, production frontend build, Rust formatting, strict Clippy, 85 Rust tests, and doc-tests. `npm run audit:dependencies` passed with zero npm vulnerabilities and no denied Rust audit findings. The companion reference worktree passed all 219 offline Pester tests, and its skeleton manifest passed `Test-Json` against this GUI schema. Independent review verified the environment, version, and Windows path fixes.

An unsigned development executable was built with a separate review application identity and opened on Windows. Native startup, tool detection, local-folder selection, embedded-manifest parsing, and minimum-version readiness were exercised using a task-authored fixture. The visual pass identified the native null-condition and checkbox layout defects covered by this batch. After rebuilding, the multiselect layout, selection-driven conditional page, and option recommendation text were visually verified. Existing cached connection status and read-only Azure scope discovery were used; no sign-in, installation, provisioning, or deployment was initiated. The isolated test app was closed afterward.

## Changes in this batch

- Require the same explicit environment for native approval and every mutating azd lifecycle command.
- Evaluate each template's prerequisite versions in Rust, including `minimumAzdVersion`; fail closed for unknown required tools and invalid ranges. Recheck before preparation and registered-project lifecycle execution.
- Preserve a validated effective template contract with each project, including direct GitHub and local imports. Bind the snapshot to template identity/revision and an integrity digest, omit sensitive defaults, and retain legacy path aliases without silently selecting duplicate registrations.
- Render multiselect fields as arrays, show option explanations, and validate declared Azure/Entra identifier inputs. Template-level identifiers currently use typed text entry; tenant-backed pickers remain future work.
- Preserve native conditional-field behavior by omitting unused serialized comparisons and tolerating legacy nulls. Show readiness even when a manifest declares only `minimumAzdVersion`.
- Separate approved preparation and terminal handoff from noninteractive preview. Retain read-only permission checks for both routes.
- Refresh reviewed configuration on an already prepared project, preserving write-only configured secrets in the project editor.
- Repair dependency audit failures with focused npm and Rust lockfile updates; use Node.js 22 for validation.
- Add an embedded manifest and authoring standard to the companion `azd-reference` skeleton, with offline contract checks and cross-repository schema validation.

## Next milestones

| Order | Deliverable | Acceptance evidence |
| --- | --- | --- |
| 1 | Finish source and approval binding | Bind local source contents and the effective manifest to approval; reject changes between inspection and execution. Bind remote sidecar manifests to immutable template content. Exercise Git redirects, inherited configuration, and DNS changes in bounded network tests. |
| 2 | One real Windows template pilot | Select a simple ARM template or make Maester's interactive route explicit. On a disposable authorized target: configure, prepare, preview where supported, deploy, reopen, verify, and clean up. Record remaining manual steps. |
| 3 | Effective optional features | Selected features determine required tools, connections, scopes, checks, and acknowledgements. Changing a feature invalidates stale approval and preview. Template scripts enforce the same choices. |
| 4 | Better admin inputs | Add one registered Entra picker and template-authored recommendation profiles with explanations of permission, cost, and component consequences. Keep unsupported lookups explicit. |
| 5 | Verification and management | Consume reference deployment receipts and validation reports. Present deployment status, verification status, outstanding actions, and ownership separately. Prove rerun and cleanup behavior before adding PIM or Device Notifications. |
| 6 | Windows beta release | Build and sign a reviewed candidate, then execute the clean-VM installer, WebView2, authentication, upgrade, and uninstall acceptance checklist. |

The GUI remains an orchestrator over public CLI contracts. Templates own infrastructure, hooks, optional feature semantics, and verification. The gallery supplies discovery and source identity; descriptive `.azd/catalog.json` commands are never an execution interface.

## Limits to preserve in product claims

A saved snapshot's SHA-256 digest checks local consistency; it is not publisher authentication and does not freeze a mutable local working tree. Current local inspection does not fingerprint every hook file. Git's DNS lookup is separate from the HTTP client's pinned-address checks. These remain source-trust hardening work before broad external-template adoption.

Browser preview is simulated. Passing offline tests or opening a development executable does not demonstrate signed installation, real broker authentication, Azure provisioning, recipient-visible notification delivery, or cleanup. Layered provisioning and other unsupported preview capabilities need explicit classification before claiming a fully guided deployment experience.
