# Architecture

The supported beta release target is Windows 10/11 on x64. macOS and Linux remain experimental source-build targets until they have their own packaging, signing, and acceptance gates.

## Runtime boundary

The Svelte renderer owns presentation and wizard state. Small stateless controllers under `src/lib/controllers` compose connection verification, deployment approval/preparation, and project lifecycle workflows without introducing another state-management layer. They call a narrow set of typed Tauri commands through `src/lib/bridge.ts`; the bridge provides realistic preview data when the UI runs outside Tauri.

Browser preview is selected only when the page is genuinely outside Tauri. If native bootstrap fails in the packaged application, the renderer shows a fatal degraded-state screen and exposes no project, authentication, or deployment actions; it never falls back to simulated success data.

Rust owns all privileged behavior:

- `catalog.rs` validates bundled and external sources, resolves GitHub revisions, and inspects local projects. Catalog HTTP and Git origins must resolve exclusively to public IP space; HTTP clients pin the accepted DNS answers, allow only bounded same-origin HTTPS redirects, verify the final response origin, and stream into capped buffers. Every external manifest sidecar declares a SHA-256 digest in its accepted catalog and its bounded raw bytes must match before parsing. Catalog entry counts, cumulative manifest bytes, local metadata files, and the in-memory inspection cache are independently bounded.
- `command_runner.rs` enforces the executable allowlist, bounds and redacts captured output, limits streamed line size, and supervises complete process trees for cancellation and timeout handling.
- `azd.rs` owns typed lifecycle and environment operations using supported public CLI commands.
- `connections.rs` checks and starts native tool authentication without handling tokens.
- `permissions.rs` runs only registered, read-only Azure and Microsoft Graph checks and returns structured pass, warning, or failure results.
- `tools.rs` detects versions, evaluates the selected template's version requirements, and exposes only vetted per-user module installers. Required unknown tools and invalid version ranges block preparation.
- `deployment.rs` verifies pinned source content, initializes projects, applies azd environment values, and records the project.
- `persistence.rs` stores non-sensitive projects, catalog trust, drafts, and operation history in SQLite.

The hidden `azd vs-server` protocol is intentionally not used.

New destinations are persisted as `attention` immediately after reservation and promoted to `ready` only after `azd init` and environment configuration complete. A failed preparation therefore remains visible under Projects with guidance to inspect and resume a usable azd project or manually remove the folder for a clean retry; the app does not guess whether partial user or hook output is safe to delete. Remote Git checkouts ignore system/global Git configuration, fetch the pinned commit and its blobs at depth one, and validate file count plus individual and total tree sizes before checkout.

The product is a management and orchestration interface over installed command-line tools, not a terminal emulator. Known workflows prefer machine-readable CLI or PowerShell output, project that output into typed Rust models, present choices as native GUI controls, and pass only validated selections back as argument arrays. Human-formatted output is diagnostic display only and is never scraped to drive application behavior. The app does not maintain a general-purpose shell or PTY; genuinely interactive, unmodeled template hooks remain an explicit external-terminal fallback. Terminal providers are selected from a fixed platform registry and resolved to absolute executable paths before launch.

Tool installation is a typed, version-pinned flow. The native layer resolves an exact PSGallery version and returns an application-owned `ToolInstallPlan`; the renderer displays its provider, source, scope, destination, version, exact command, and documentation. Confirmation re-resolves the package, presents a native OS dialog, and issues a one-use approval bound to the module ID and exact version. Only that approval can start the corresponding registered `CommandSpec`; tool IDs that are not in the native non-elevated registry cannot produce either a plan or an executable command.

The Windows Azure CLI provider invokes the official `az.cmd` shim because the MSI also places an extensionless Unix shell wrapper on `PATH`. Renderer and manifest data cannot select this shim, and command-shell metacharacters are rejected from its argument array before process creation. Before each command starts, the allowlisted executable is resolved to an absolute path; that exact path is executed, recorded in start events, and shown with detected tools. Registered background commands use the Windows no-console process flag so normal checks, authentication, and lifecycle operations do not open a console window. Every spawned command is attached to a Windows Job Object or Unix process group so cancellation, timeout, and runner shutdown terminate descendant processes as well as the direct child.

When supported by the installed azd version, an explicit tool refresh also consumes the public `azd tool list/check --output json` contracts. The slower network-backed update scan is not part of application bootstrap. It enriches Azure CLI status with azd's installed and latest versions while preserving the direct executable probe as a fallback. Available updates are informational and do not block deployment readiness. Because azd 1.30 dry-run reports the action but not its selected package-manager strategy, elevation, or destination, system-tool installs and upgrades remain user-directed and the app exposes a copyable dry-run command instead of executing them.

## Deployment flow

1. Select a bundled catalog entry, external catalog entry, direct GitHub repository, or local folder.
2. Resolve the effective manifest and compute the wizard pages.
3. Verify prerequisite and connection state.
4. Collect environment and template values. Sensitive values are not placed in drafts or logs.
5. Review the exact values and explicitly approve pinned template code execution for the current session.
6. Run manifest-requested registered permission checks and stop on required failures.
7. Resolve a remote source to a full commit and cache that exact checkout.
8. Run `azd init` for new projects or register an environment for an existing project, then apply approved values through a temporary `azd env set --file` input. Ordinary and sensitive environment configuration use the same transport so values are not exposed in process arguments.
9. For guided templates, run `azd provision --preview --no-prompt`. For templates requiring interactive input, prepare and hand off directly to a validated external terminal after permission checks; a noninteractive preview is not a prerequisite for this route.
10. Run `azd up --no-prompt` for guided templates, then refresh project state. Terminal handoff does not establish deployment success.

Mutating lifecycle commands require an explicit, validated environment. Approval and the CLI argument use that same environment; neither can fall back to azd's current default. Native preparation and subsequent registered-project lifecycle operations reevaluate the saved template's required tool versions.

Project registration stores the effective catalog entry and manifest with a SHA-256 integrity digest and binds it to the project's template ID and revision. Sensitive defaults are removed. Local and direct GitHub projects therefore retain their guided configuration across restarts and catalog changes. External catalog snapshots also retain the declared sidecar digest after native verification, so a deployment uses the accepted contract instead of refetching a mutable manifest. Before new external-gallery initialization, the pinned checkout must contain byte-identical embedded metadata with the same effective contract, including detected interactive hooks. This runs before destination creation or project registration, and initialization uses that retained checkout. These digests are not publisher signatures; old registrations are not retroactively verified and later working-tree edits remain possible. Legacy unsigned external snapshots fail closed and require a fresh review. Legacy projects use a catalog fallback only when its revision exactly matches their recorded revision. Local source changes still require source review.

The consent gate precedes project materialization because both `azd init` and lifecycle commands can execute template hooks. Renderer acknowledgement is only explanatory: Rust displays a native confirmation and issues a five-minute, one-use approval bound to the action, project path, environment, template ID, and revision. Resource removal receives its own destructive confirmation. `azd provision --preview` is not treated as read-only: azd does not provide a no-hooks preview flag, so a preview can change local files, Azure resources, or directory objects. Every preview terminal outcome is therefore classified as `possiblyModified`, including success. Opening an existing project creates a fresh session consent gate before preview, provision, deployment, or resource removal; read-only project refresh remains available without that approval.

Permission-check IDs are an application-owned registry. Catalogs can select a registered ID but cannot provide a command, script, URL, or executable argument. V1 can perform a live ARM subscription read, a Microsoft Graph organization read, and a Microsoft Graph PowerShell delegated-scope comparison. These checks do not capture tokens and do not mutate Azure or Microsoft Entra resources.

Only one process operation is represented by an operation ID. Terminal lifecycle events include a structured resource state: `changed`, `unchanged`, `possiblyModified`, or `unknown`. Failed or cancelled mutating operations are conservatively marked `possiblyModified` because Azure-side work already accepted by the service may continue after the local child process stops. The UI presents that guidance and can retry only the same registered lifecycle action; templates cannot inject retry commands or arguments. Output remains a read-only transcript: ANSI formatting is removed at the native redaction boundary, tool output is shown without repetitive per-line metadata, and application-owned system events retain timestamps.

Interactive hooks use an explicit external-terminal escape hatch. Rust canonicalizes the selected folder, requires `azure.yaml`, validates the environment name, and chooses a terminal from a fixed platform-owned provider list. The terminal opens at the project directory without executing renderer- or manifest-supplied text. The UI separately displays the app-constructed interactive `azd up` command for the user to copy.

## Authentication

The Connections center invokes each provider's supported login:

- `azd auth login`
- `az login`
- `Connect-AzAccount`
- `Connect-MgGraph`

The provider retains its credentials. The app reads only account/tenant/status output and warns when connected tenants do not align. After a successful login process, the app keeps that provider in a checking state and performs bounded status retries so a briefly delayed credential cache does not produce a false disconnected tile or a second login. The public azd auth-status expiry is preserved and an elapsed timestamp changes the connection to `expired`, which reuses the normal reconnect gate. Microsoft Graph PowerShell status also preserves the delegated scope names returned by `Get-MgContext`; the wizard compares those names case-insensitively with manifest-requested Graph scopes and sequences a reconnect when grants are missing. A standalone Graph connection omits `-Scopes` instead of passing an empty value, while a template-guided connection passes only the manifest-requested scope names through a process-private environment value. Providers whose caches do not expose comparable scope names remain connection-status checks and are verified by registered preflight providers where applicable. Interactive sign-in uses only the provider's normal cached broker or system-browser flow.

After the deployment target is selected, connection checks and reconnects receive a validated tenant/subscription target. azd, Azure CLI, Azure PowerShell, and Microsoft Graph PowerShell receive their supported tenant parameters; Azure CLI and Azure PowerShell status are checked against the selected subscription. App-owned Azure CLI reads pass `--subscription` explicitly. The app never runs `az account set`, so it does not silently change the user's global Azure CLI default.

Future app-owned MSAL authentication can be introduced behind the same provider boundary, using WAM on Windows and a system-browser loopback redirect elsewhere.

## Persistence

SQLite is stored in the Tauri application-data directory. It contains project metadata, catalog sources and trust decisions, non-sensitive wizard drafts, typed appearance and navigation settings, and operation history, including structured terminal outcomes. Draft persistence accepts only typed standard or manifest-declared keys; Rust resolves sensitivity from bundled, accepted, or active inspected manifests and removes stale unsafe values when a draft is loaded. Drafts are namespaced by a versioned digest of the reviewed template entry (including repository, revision, subdirectory, and effective manifest). The frontend captures that identity with each queued save; changed identities and old ID-only drafts are safe misses rather than automatic restores. Legacy rows are retained until local app data is cleared. This digest provides local consistency, not publisher authentication. A failed or cancelled operation marks its matching project as needing attention across restarts. azd continues to own `.azure` environment files.

The Settings page can clear all app-owned persisted state and cached template material after a native confirmation. Clearing is refused while a command is active, uses SQLite secure deletion followed by WAL truncation and compaction, and invalidates in-memory trust and approval state. It deliberately does not delete project directories or credentials owned by azd, Azure CLI, or PowerShell.

Pinned remote templates are fetched into a fresh temporary checkout for each materialization. The commit is verified after checkout, the checkout remains alive only through `azd init`, and it is then deleted. Executable template working trees are never reused across deployments.

Project workspaces refresh their current environment through the public `azd show --output json` contract. The renderer projects that untrusted JSON into typed service summaries, accepts only HTTPS endpoints, and creates Azure portal links only for validated ARM resource IDs. Raw state remains available as an advanced diagnostic view.
Environment reads are applied only to the workspace request that started them. The UI retains pending configuration, raw-variable, creation, and selection promises per project path across navigation; reopening that project waits for the mutation before reading state. Save reloads preserve newer unsaved edits. This coordinates operations within the current app window; external CLI edits remain independently mutable.

Configuration reads distinguish idle, loading, ready, and failed states. Projects without an environment show selection guidance; failed reads hide the editor and offer a retry. Preview, provisioning, and deployment require successfully read configuration. Teardown requires a selected environment with no pending workspace load or local environment mutation, but does not require readable or complete deployment configuration, so configuration failures do not prevent cleanup.

Saved contracts requiring an interactive terminal keep that route after the project is reopened. The workspace offers terminal handoff, and native lifecycle execution rejects preview, up, provision, deploy, and down through the output-only runner before checking prerequisites or consuming approval. Environment refresh remains available. Legacy projects without a resolvable contract retain the existing route; later edits to project files are not frozen or reclassified by this guard.

## Project validation reports

The Overview panel reads only `reports/deployment-validation.json` beneath a registered project. This read invokes no CLI, hooks, authentication, or cloud operation and does not update deployment state. The native reader accepts the reference `azdDeploymentValidation` version `1.0` envelope, enforces a 1 MiB read bound and 500-check limit, and rejects links/reparse points along the fixed report path. It checks reported totals against check statuses and outcome precedence against the reference producer.

Only fixed enums, counts, durations, parsed timestamps, and advisory environment-name binding reach the UI. Raw check IDs, prose, evidence, expected/actual values, metadata, requirements, and target identifiers are omitted; checks are displayed by number and category. The local file is not independently authenticated. A matching environment name does not establish the Azure target, source revision, freshness, delivery, or cleanup. Missing or invalid reports leave the workspace usable. Evidence binding remains roadmap work.

## Project deployment receipts

The Overview panel also reads `reports/deployment-receipt.json` beneath a registered project, matching the candidate reference writer's default path. The native reader uses the same bounded, link-aware file boundary as validation reports. It accepts the existing reference receipt v1 envelope and exposes only generation time, plan/enforce mode, five summary counters, and counts of referenced artifacts and outstanding actions. Artifact paths, arbitrary action text, template labels, and all `details` values are omitted; referenced files are never followed.

A plan cannot report applied actions. An enforce receipt records the producer's account of actions and is not independent deployment proof. The panel does not change project state or claim environment, target, source, delivery, or cleanup binding. Missing receipts are normal for templates that have not adopted the candidate writer. Producer adoption and solution-owned evidence binding remain separate milestones.
