# Windows readiness and template adoption

Implementation follow-up to the 12 September 2026 review. Local development checks, signed-package verification, and live CLI/template evidence are tracked separately from full Windows GUI acceptance.

Use [EXECUTION-PLAN.md](EXECUTION-PLAN.md) for ordered task packets, model assignments,
acceptance checks, and bounded handoffs. This roadmap retains the historical context.

The [20 September acceptance follow-up](windows-acceptance-20260920.md) records the named starter, native draft fixes, historical installer failure, and remaining authentication/upgrade gates.

## Signed packaging evidence

The first signed candidate exposed an installation defect: Tauri changed the executable's installer-type marker after it had been signed. The outer installer and portable signatures passed, but the installed executable failed Authenticode with `HashMismatch`. [PR #89](https://github.com/nathanmcnulty/azd-gui/pull/89) moved signing into Tauri's supported hook, after the marker change, and added an installed-payload gate before artifact upload.

[The replacement signing run](https://github.com/nathanmcnulty/azd-gui/actions/runs/35481838863) passed at `cc3d9540be2bfccb4eb83c5d4efa169446ddffcc`: dependency audits, frontend/native checks, signing, installed-payload hash and signature/version verification, and uninstall on the runner. Independent download verification and non-elevated installation in Windows Sandbox also passed; the installed executable matched the recorded payload and had a valid signature from the independently checked signing profile. Thirteen local PowerShell regression tests cover the verifier and signing hook. This establishes packaging evidence for that exact candidate, not visual, authentication, deployment, or upgrade acceptance, and does not automatically cover later application changes. No tag or public release was created.

After installing Microsoft's signed offline WebView2 Runtime in the disposable sandbox, the candidate passed a non-elevated startup smoke: a responding window, a live child WebView2 process, and newly written local database files. The same standard user uninstalled it successfully, and the executable was absent afterward. A missing-runtime launch was observed separately, but its bootstrap UI was not inspected and is not counted as visual acceptance. Sandbox evidence remains private; the disposable session and guest-only account were removed afterward.

## ARM-only template pilot

An explicitly authorized pilot used the reference skeleton to create only an empty resource group in `westus2`, validate it, and delete it. The first run found a context-validation defect: postprovision checked Azure CLI's global default rather than the environment's subscription. [Reference PR #31](https://github.com/nathanmcnulty/azd-reference/pull/31) scopes that read explicitly and accepts normal azd environments without a tenant variable, while still checking a supplied tenant.

The repeat at reference commit `a1a594f7a33f2362f4fa613fd30e6edc32825269` passed environment creation, preview, provisioning, all three read-only validation checks, and `azd down`. Synthetic delivery remained intentionally skipped. Independent reads confirmed no resources inside the group before cleanup and the group absent afterward. The repeat preserved the CLI default it found; offline regressions cover a different default and reject subscription/tenant mismatches. Private lifecycle reports retain exact targets and command evidence. This proves the minimal CLI/template lifecycle, not GUI deployment, application workloads, or Maester permissions and cleanup.

## Local validation

On 19 September, the dependency refresh patched the `devalue` malformed-input denial-of-service advisory with 5.9.2 and RUSTSEC-2026-0285 with `rustls` 0.23.45 (plus its `rustls-webpki` 0.103.15 dependency). The npm and strict Cargo audits passed with the existing documented upstream maintenance exceptions unchanged.

An opt-in installed-CLI regression also passed with azd 1.34.0: `cargo test --manifest-path src-tauri/Cargo.toml installed_azd_prepares_the_selected_destination -- --ignored`. It prepares a temporary hook-free local template, writes the selected environment, and verifies both `azure.yaml` and `.azure/dev/.env` at the selected destination. It does not authenticate, provision, or deploy. This matters because [azd changed the general template initialization directory behavior](https://devblogs.microsoft.com/azure-sdk/azure-developer-cli-azd-april-2026/); the GUI uses a staged local template path, which the test exercises directly.

On Windows with Node.js 22.23.2, `npm run verify` passed: zero Svelte/TypeScript errors or warnings, 106 frontend tests, production frontend build, Rust formatting, strict Clippy, 130 Rust tests (plus three opt-in installed-tool checks run separately), and doc-tests. `npm run audit:dependencies` passed with zero npm vulnerabilities and no denied Rust audit findings. The companion reference passed all 229 Pester tests on Windows and Linux, and its skeleton manifest passed `Test-Json` against this GUI schema. Independent review verified the environment, version, and Windows path fixes.

[PR #90](https://github.com/nathanmcnulty/azd-gui/pull/90) subsequently passed 110 frontend tests, Svelte/TypeScript checks, and the production build; the final target-guard follow-up passed checks and 13 focused tests. Deferred-response regressions reproduce both prior environment-state races. The reference pilot fix passed 230 Pester tests locally and in Windows/Linux CI.

An unsigned development executable was built with a separate review application identity and opened on Windows. Native startup, tool detection, local-folder selection, embedded-manifest parsing, and minimum-version readiness were exercised using a task-authored fixture. The visual pass identified the native null-condition and checkbox layout defects covered by this batch. After rebuilding, the multiselect layout, selection-driven conditional page, and option recommendation text were visually verified. Existing cached connection status and read-only Azure scope discovery were used; no sign-in, installation, provisioning, or deployment was initiated. The isolated test app was closed afterward.

The security-group picker was verified with a cached, explicitly selected Azure CLI tenant/subscription through the native Windows command runner. Account and Graph organization checks matched; the bounded group query passed without sign-in or permission changes. Browser preview verified explicit selection, and regression tests cover stale queries and target mismatch. Query parameters use a temporary JSON file so Windows command-shell metacharacters remain data. This is read-only input validation, not deployment proof.

The receipt compatibility fixture was generated by the candidate deployment-receipt 0.1.1 writer in [azd-reference PR #28](https://github.com/nathanmcnulty/azd-reference/pull/28). The final writer output matches the checked-in fixture exactly; the GUI native reader consumes it, and its Windows junction regression passes. This is local producer/reader compatibility evidence; no solution has adopted the writer yet.

## Changes in this batch

- Require the same explicit environment for native approval and every mutating azd lifecycle command.
- Evaluate each template's prerequisite versions in Rust, including `minimumAzdVersion`; fail closed for unknown required tools and invalid ranges. Recheck before preparation and registered-project lifecycle execution.
- Preserve a validated effective template contract with each project, including direct GitHub and local imports. Bind the snapshot to template identity/revision and an integrity digest, omit sensitive defaults, and retain legacy path aliases without silently selecting duplicate registrations.
- Render multiselect fields as arrays, show option explanations, and validate declared Azure/Entra identifier inputs. Typed entry remains available alongside the first tenant-backed security-group picker.
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
- Resolve optional tools, connections/scopes, and registered read-only checks from explicit boolean feature choices. Put controlling feature groups before readiness gates, invalidate review on edits, and discard stale check results. Maester's Exchange module is required only when Exchange coverage is enabled; its baseline ARM/Graph access remains unconditional. Native lifecycle execution re-reads controller values from the selected environment before tool checks; this does not freeze externally mutable project state.

- Replace raw wizard keys in the final review with visible configuration groups, field labels, readable option names, Yes/No choices, and explicit unset/hidden states. Include the Azure tenant and resource group in the target summary. Direct Edit buttons return to the relevant earlier section and clear execution approval; normal readiness gates remain in place.

- Explain each disabled Continue action beside the button, with accessible live guidance for missing inputs, tools, connections, target mismatches, source trust, preview, and execution approval. One readiness decision drives both the instruction and button state.

- Show when saved wizard choices are restored and ignore late restores after editing, navigation, or reopening. Capture non-sensitive drafts at edit time, serialize writes, and flush pending edits when leaving or switching wizards. Report load/save failures without raw diagnostics and allow retrying a failed save. Discard stale source inspection, folder selection, and Azure discovery results; preserve explicitly chosen local sources. External-gallery navigation skips local-folder inspection.

- Isolate saved wizard choices by the reviewed template identity, including revision and effective manifest. Preserve old ID-only rows without restoring them into a potentially changed template; native key allowlisting and secret filtering remain authoritative.

- Read aggregate deployment receipts separately from validation and deployment state. Show plans, recorded actions, warnings, and outstanding action counts; discard unstructured text and never follow artifact links. A reference-owned candidate writer supplies the default path, but no template adoption or deployment proof is implied.

- Keep project workspace reads and local environment mutations tied to the request that started them. Reopening a project waits for pending configuration, raw-variable, create, or select operations before reading state. Preserve edits made during configuration saves, suppress stale workspace callbacks, and show generic save failures without raw diagnostics.

- Distinguish missing environments from loading and failed configuration reads, offer direct retry, and block deployment actions until saved configuration is available. Teardown retains a separate environment gate so missing deployment configuration does not strand cleanup.

- Block preview, provisioning, and deployment while manifest edits are unsaved, with guidance back to the editor. Save persists the choices; Discard restores the last read saved values without writing to azd. Guard in-app navigation, refresh, environment changes, terminal launch, and lifecycle actions while manifest or advanced-variable input is unsaved. Advanced variables have an explicit discard action and invalidate previews. Drafts remain in memory; process exit and external CLI changes are not covered.

- Honor the saved interactive-terminal requirement after reopening registered projects. Their workspace offers a selected-environment terminal handoff; native execution rejects preview, up, provision, deploy, and down through the output-only route before running prerequisite commands or consuming approval. Refresh remains available. This enforces known saved contracts and does not detect later edits to mutable project files.

- Read workspace resources for the environment actually discovered as current. Preserve the initiating project and environment while lifecycle startup is pending, so navigation cannot attach an old preview/result to a different workspace or clear its deployment result.

## Next milestones

| Order | Deliverable | Acceptance evidence |
| --- | --- | --- |
| 1 | Complete external-template provenance | New external-gallery preparation binds reviewed sidecar bytes and the effective contract to the pinned template before any initialization. Next define publisher trust and evidence for old project registrations. Public GitHub compatibility is verified against azd-reference. Specify approval semantics for intentionally mutable existing-project working trees. |
| 2 | One real Windows template pilot | The resource-group-only reference pilot passes through the CLI with independent cleanup verification. The named Azure deployment check starter is now bundled at a reviewed reference revision. Configure, prepare, cancel/retry preview, deploy, reopen, verify, and cleanup passed through the native Windows development GUI with cached authentication and independent Azure absence verification. Repeat final signed acceptance separately. Keep Maester's interactive and directory-permission acceptance separate. |
| 3 | Effective optional features | Boolean feature conditions now select required tools, connections/scopes, and registered checks; edits invalidate review/preview. Extend proven template mappings and per-feature acknowledgements as more projects are adopted. Broader expressions and immutable existing-project approval semantics remain future work. |
| 4 | Better admin inputs | The wizard now offers a registered security-group picker for Maester's dashboard, with explicit selection and target-change invalidation. Expand proven profiles and inputs with permission, cost, and component explanations as templates are adopted. Existing-project editing and unsupported lookups retain manual ID entry. |
| 5 | Verification and management | Validation report reading is implemented. A candidate reference receipt writer and aggregate GUI reader now use reports/deployment-receipt.json. Next define explicit deployment-operation outcomes in a real solution, adopt the writer, and bind evidence to the exact deployment, target, and source. Do not count validation checks as applied deployment changes. Present outstanding actions and ownership separately; prove rerun and cleanup behavior before adding PIM or Device Notifications. |
| 6 | Windows beta release | Signed packaging, non-elevated installation, native startup smoke with WebView2, and standard-user uninstall passed for the recorded candidate. Signed 0.1.1 visual UI, corrected-baseline upgrade, legacy data retention, local-data clearing, and standard-user uninstall passed; missing-WebView2 UX was observed on the earlier verified baseline. Complete clean-VM interactive authentication and resolve the historical public installer limitation before publication. |

The GUI remains an orchestrator over public CLI contracts. Templates own infrastructure, hooks, optional feature semantics, and verification. The gallery supplies discovery and source identity; descriptive `.azd/catalog.json` commands are never an execution interface.

## Limits to preserve in product claims

A saved snapshot's SHA-256 digest checks local consistency; it is not publisher authentication. External sidecar bytes are checked against a digest in the accepted catalog and are not fetched again during execution, and new external-gallery preparation requires matching embedded metadata in the pinned template. This does not retroactively verify old registrations or bind later mutable project edits. New local-template preparation uses a reviewed, staged source tree, while an existing-project working tree remains intentionally mutable after approval. Clean-VM interactive authentication and the historical public installer upgrade limitation remain open; signed 0.1.1 offline acceptance passed. Git address pinning uses the documented [http.curloptResolve setting](https://git-scm.com/docs/git-config); private hosts, redirecting clone URLs, inherited proxies, and implicit credential helpers are intentionally unsupported by this public-template download path.

Browser preview is simulated. Passing offline tests or opening a development executable does not demonstrate signed installation, real broker authentication, Azure provisioning, recipient-visible notification delivery, or cleanup. Layered provisioning and other unsupported preview capabilities need explicit classification before claiming a fully guided deployment experience.

## First Maester pilot

The bundled entry targets `45f839bc2c335cd1c7fa4c40ffe9c6978f715362`, `automation-account`. Its variables and feature defaults agree with that revision's interactive wizard, and it pins the signed deployment receipt and validation components. Preserve these defaults in the catalog; use explicit choices for a least-privilege pilot.

1. Use an explicitly authorized disposable subscription/environment and a new resource group with no existing Automation Account. Redeploying an existing account can remove its delete lock and all job schedules.
2. Select Minimal, dashboard off, Exchange/Teams/Azure off, and an empty mail recipient. Review the eight baseline managed-identity Graph application roles, which still apply.
3. Confirm the prepared azd environment contains the reviewed choices before invoking interactive hooks. Read-only ARM/Graph checks do not establish write or role-assignment authority; confirm the operator's required permissions separately.
4. Run through the external terminal, capture actual hook/setup outcomes, reopen the project, and verify the resulting resources and test run. Record skipped or failed setup actions separately from provisioning success.
5. Exercise `azd down` against the same disposable environment and verify the template's best-effort identity, Exchange, and scoped-role cleanup. A successful CLI exit alone is insufficient evidence of complete cleanup.

This pilot has not been run. It requires a selected cloud target and explicit deployment authorization.
