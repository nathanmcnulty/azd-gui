# Windows readiness and template adoption

Implementation follow-up to the 12 September 2026 review. This branch establishes a local development baseline; signed Windows acceptance and a real deployment pilot remain separate completion gates.

## Local validation

An opt-in installed-CLI regression also passed with azd 1.34.0: `cargo test --manifest-path src-tauri/Cargo.toml installed_azd_prepares_the_selected_destination -- --ignored`. It prepares a temporary hook-free local template, writes the selected environment, and verifies both `azure.yaml` and `.azure/dev/.env` at the selected destination. It does not authenticate, provision, or deploy. This matters because [azd changed the general template initialization directory behavior](https://devblogs.microsoft.com/azure-sdk/azure-developer-cli-azd-april-2026/); the GUI uses a staged local template path, which the test exercises directly.

On Windows with Node.js 22.23.2, `npm run verify` passed: zero Svelte/TypeScript errors or warnings, 59 frontend tests, production frontend build, Rust formatting, strict Clippy, 100 Rust tests (plus two opt-in installed-tool checks run separately), and doc-tests. `npm run audit:dependencies` passed with zero npm vulnerabilities and no denied Rust audit findings. The companion reference worktree passed all 219 offline Pester tests, and its skeleton manifest passed `Test-Json` against this GUI schema. Independent review verified the environment, version, and Windows path fixes.

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
- Bind local preparation approval to the reviewed source tree and embedded manifest. Stage local templates into a verified private copy; reject links, changed files, and oversized trees. Exclude only root `.git` and `.azure` metadata/runtime state.
- Pin Git transport to validated public DNS addresses, reject redirects, and isolate inherited Git, credential, and proxy configuration. An offline loopback-server regression verifies pinning and redirect rejection. HTTP catalog requests also bypass inherited proxies.
- Preserve prepared environments across target edits and environment changes, tracking written configuration separately for each environment so hidden values can be cleared without reinitializing the project.
- Explain Maester's Graph, Entra, Exchange, and Azure role changes beside feature choices and surface the existing advisory directory-write check.
- Verify external sidecar bytes against the catalog's `manifestSha256` before parsing, retain the digest with accepted contracts, and require old unsigned external snapshots to be reviewed again. Check the full HTTPS origin before making a sidecar request.
- Offer explicit template-authored recommendation profiles. Maester includes a Minimal pilot starting point with service extensions and notifications disabled; the eight baseline Graph application roles still apply. Profiles preserve unrelated target values and invalidate prior wizard review.
- Detect project, service, and layer hooks from YAML structure and persist the interactive requirement in the saved contract. Interactive preflight uses terminal preparation as the primary route.
- Exercise the restricted Git path against the merged reference commit. Fetch blobs together with the shallow commit: blobless cloning made size inspection perform individual lazy fetches and time out; the corrected public-reference checkout completed in about two seconds. Git fetch remains time-bounded, and file/count/size limits are enforced before checkout; this is not a byte quota on the downloaded pack.
- Read the reference version 1.0 validation report from registered projects and display it separately from deployment state. Bound reads to 1 MiB and 500 checks, reject linked paths, discard free text and target identifiers, and validate totals and outcomes against check statuses using the reference producer's precedence. A real offline reference plan is checked in as the compatibility fixture. Environment-name matching is advisory and does not establish target identity, provenance, or freshness.
- Label interactive wizard steps as preparation and terminal handoff, matching the actual execution route.

## Next milestones

| Order | Deliverable | Acceptance evidence |
| --- | --- | --- |
| 1 | Complete external-template provenance | Bind remote sidecar manifests to immutable template content and define publisher trust. External sidecar bytes now bind to the accepted catalog SHA-256 digest, but not yet to repository content. Public GitHub compatibility is verified against azd-reference. Specify approval semantics for intentionally mutable existing-project working trees. |
| 2 | One real Windows template pilot | Select a simple ARM template or make Maester's interactive route explicit. On a disposable authorized target: configure, prepare, preview where supported, deploy, reopen, verify, and clean up. Record remaining manual steps. |
| 3 | Effective optional features | Selected features determine required tools, connections, scopes, checks, and acknowledgements. Changing a feature invalidates stale approval and preview. Template scripts enforce the same choices. |
| 4 | Better admin inputs | Add one registered Entra picker. Template-authored recommendation profiles are implemented; expand proven profiles with permission, cost, and component explanations as templates are adopted. Keep unsupported lookups explicit. |
| 5 | Verification and management | Validation report reading is implemented. Next consume deployment receipts and bind evidence to the exact deployment, target, and source. Present outstanding actions and ownership separately; prove rerun and cleanup behavior before adding PIM or Device Notifications. |
| 6 | Windows beta release | Build and sign a reviewed candidate, then execute the clean-VM installer, WebView2, authentication, upgrade, and uninstall acceptance checklist. |

The GUI remains an orchestrator over public CLI contracts. Templates own infrastructure, hooks, optional feature semantics, and verification. The gallery supplies discovery and source identity; descriptive `.azd/catalog.json` commands are never an execution interface.

## Limits to preserve in product claims

A saved snapshot's SHA-256 digest checks local consistency; it is not publisher authentication. External sidecar bytes are checked against a digest in the accepted catalog and are not fetched again during execution, but the sidecar is not yet bound to repository content. New local-template preparation uses a reviewed, staged source tree, while an existing-project working tree remains intentionally mutable after approval. Signed Windows acceptance remains open. Git address pinning uses the documented [http.curloptResolve setting](https://git-scm.com/docs/git-config); private hosts, redirecting clone URLs, inherited proxies, and implicit credential helpers are intentionally unsupported by this public-template download path.

Browser preview is simulated. Passing offline tests or opening a development executable does not demonstrate signed installation, real broker authentication, Azure provisioning, recipient-visible notification delivery, or cleanup. Layered provisioning and other unsupported preview capabilities need explicit classification before claiming a fully guided deployment experience.

## First Maester pilot

The bundled entry targets `34f4bac7b69ea2a6746705a91f35ef89ab346034`, `automation-account`. Its variables and feature defaults agree with that revision's interactive wizard. Preserve these defaults in the catalog; use explicit choices for a least-privilege pilot.

1. Use an explicitly authorized disposable subscription/environment and a new resource group with no existing Automation Account. Redeploying an existing account can remove its delete lock and all job schedules.
2. Select Minimal, dashboard off, Exchange/Teams/Azure off, and an empty mail recipient. Review the eight baseline managed-identity Graph application roles, which still apply.
3. Confirm the prepared azd environment contains the reviewed choices before invoking interactive hooks. Read-only ARM/Graph checks do not establish write or role-assignment authority; confirm the operator's required permissions separately.
4. Run through the external terminal, capture actual hook/setup outcomes, reopen the project, and verify the resulting resources and test run. Record skipped or failed setup actions separately from provisioning success.
5. Exercise `azd down` against the same disposable environment and verify the template's best-effort identity, Exchange, and scoped-role cleanup. A successful CLI exit alone is insufficient evidence of complete cleanup.

This pilot has not been run. It requires a selected cloud target and explicit deployment authorization.
