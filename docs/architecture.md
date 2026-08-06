# Architecture

## Runtime boundary

The Svelte renderer owns presentation and wizard state. It calls a narrow set of typed Tauri commands through `src/lib/bridge.ts`; the bridge provides realistic preview data when the UI runs outside Tauri.

Rust owns all privileged behavior:

- `catalog.rs` validates bundled and external sources, resolves GitHub revisions, and inspects local projects.
- `command_runner.rs` enforces the executable allowlist, streams redacted operation events, and manages cancellation.
- `azd.rs` defines the `AzdClient` lifecycle interface using supported public CLI commands.
- `connections.rs` checks and starts native tool authentication without handling tokens.
- `permissions.rs` runs only registered, read-only Azure and Microsoft Graph checks and returns structured pass, warning, or failure results.
- `tools.rs` detects versions and exposes only vetted per-user module installers.
- `deployment.rs` verifies pinned source content, initializes projects, applies azd environment values, and records the project.
- `persistence.rs` stores non-sensitive projects, catalog trust, drafts, and operation history in SQLite.

The hidden `azd vs-server` protocol is intentionally not used.

Tool installation is a two-step typed flow. The renderer requests an application-owned `ToolInstallPlan` and displays its provider, source, scope, destination, exact command, and documentation. A separate confirmation starts the corresponding registered `CommandSpec`; tool IDs that are not in the native non-elevated registry cannot produce either a plan or an executable command.

The Windows Azure CLI provider invokes the official `az.cmd` shim because the MSI also places an extensionless Unix shell wrapper on `PATH`. Renderer and manifest data cannot select this shim, and command-shell metacharacters are rejected from its argument array before process creation.

When supported by the installed azd version, an explicit tool refresh also consumes the public `azd tool list/check --output json` contracts. The slower network-backed update scan is not part of application bootstrap. It enriches Azure CLI status with azd's installed and latest versions while preserving the direct executable probe as a fallback. Available updates are informational and do not block deployment readiness. Because azd 1.30 dry-run reports the action but not its selected package-manager strategy, elevation, or destination, system-tool installs and upgrades remain user-directed and the app exposes a copyable dry-run command instead of executing them.

## Deployment flow

1. Select a bundled catalog entry, external catalog entry, direct GitHub repository, or local folder.
2. Resolve the effective manifest and compute the wizard pages.
3. Verify prerequisite and connection state.
4. Collect environment and template values. Sensitive values are not placed in drafts or logs.
5. Run manifest-requested registered permission checks and stop on required failures.
6. Resolve a remote source to a full commit and cache that exact checkout.
7. Run `azd init` for new projects or register an environment for an existing project.
8. Apply approved values with `azd env set`.
9. Run `azd provision --preview --no-prompt`.
10. Run `azd up --no-prompt`, stream output, and refresh project state.

Permission-check IDs are an application-owned registry. Catalogs can select a registered ID but cannot provide a command, script, URL, or executable argument. V1 can perform a live ARM subscription read, a Microsoft Graph organization read, and a Microsoft Graph PowerShell delegated-scope comparison. These checks do not capture tokens and do not mutate Azure or Microsoft Entra resources.

Only one process operation is represented by an operation ID. Terminal lifecycle events include a structured resource state: `changed`, `unchanged`, `possiblyModified`, or `unknown`. Failed or cancelled mutating operations are conservatively marked `possiblyModified` because Azure-side work already accepted by the service may continue after the local child process stops. The UI presents that guidance and can retry only the same registered lifecycle action; templates cannot inject retry commands or arguments.

Interactive hooks use an explicit external-terminal escape hatch. Rust canonicalizes the selected folder, requires `azure.yaml`, validates the environment name, and chooses a terminal from a fixed platform-owned provider list. The terminal opens at the project directory without executing renderer- or manifest-supplied text. The UI separately displays the app-constructed interactive `azd up` command for the user to copy.

## Authentication

The Connections center invokes each provider's supported login:

- `azd auth login`
- `az login`
- `Connect-AzAccount`
- `Connect-MgGraph`

The provider retains its credentials. The app reads only account/tenant/status output and warns when connected tenants do not align. The public azd auth-status expiry is preserved and an elapsed timestamp changes the connection to `expired`, which reuses the normal reconnect gate. Microsoft Graph PowerShell status also preserves the delegated scope names returned by `Get-MgContext`; the wizard compares those names case-insensitively with manifest-requested Graph scopes and sequences a reconnect when grants are missing. A standalone Graph connection omits `-Scopes` instead of passing an empty value, while a template-guided connection passes only the manifest-requested scope names through a process-private environment value. Providers whose caches do not expose comparable scope names remain connection-status checks and are verified by registered preflight providers where applicable. Device-code login is an explicit fallback.

Future app-owned MSAL authentication can be introduced behind the same provider boundary, using WAM on Windows and a system-browser loopback redirect elsewhere.

## Persistence

SQLite is stored in the Tauri application-data directory. It contains project metadata, catalog sources and trust decisions, non-sensitive wizard drafts, typed appearance and navigation settings, and operation history, including structured terminal outcomes. Draft persistence accepts only typed standard or manifest-declared keys; Rust resolves sensitivity from bundled, accepted, or active inspected manifests and removes stale unsafe values when a draft is loaded. A failed or cancelled operation marks its matching project as needing attention across restarts. azd continues to own `.azure` environment files.

The template cache is content-addressed by template ID and commit. A cached Git checkout must resolve to the catalog commit before it is used.

Project workspaces refresh their current environment through the public `azd show --output json` contract. The renderer projects that untrusted JSON into typed service summaries, accepts only HTTPS endpoints, and creates Azure portal links only for validated ARM resource IDs. Raw state remains available as an advanced diagnostic view.
