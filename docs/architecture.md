# Architecture

## Runtime boundary

The Svelte renderer owns presentation and wizard state. It calls a narrow set of typed Tauri commands through `src/lib/bridge.ts`; the bridge provides realistic preview data when the UI runs outside Tauri.

Rust owns all privileged behavior:

- `catalog.rs` validates bundled and external sources, resolves GitHub revisions, and inspects local projects.
- `command_runner.rs` enforces the executable allowlist, streams redacted operation events, and manages cancellation.
- `azd.rs` defines the `AzdClient` lifecycle interface using supported public CLI commands.
- `connections.rs` checks and starts native tool authentication without handling tokens.
- `tools.rs` detects versions and exposes only vetted per-user module installers.
- `deployment.rs` verifies pinned source content, initializes projects, applies azd environment values, and records the project.
- `persistence.rs` stores non-sensitive projects, catalog trust, drafts, and operation history in SQLite.

The hidden `azd vs-server` protocol is intentionally not used.

## Deployment flow

1. Select a bundled catalog entry, external catalog entry, direct GitHub repository, or local folder.
2. Resolve the effective manifest and compute the wizard pages.
3. Verify prerequisite and connection state.
4. Collect environment and template values. Sensitive values are not placed in drafts or logs.
5. Resolve a remote source to a full commit and cache that exact checkout.
6. Run `azd init` for new projects or register an environment for an existing project.
7. Apply approved values with `azd env set`.
8. Run `azd provision --preview --no-prompt`.
9. Run `azd up --no-prompt`, stream output, and refresh project state.

Only one process operation is represented by an operation ID. Cancellation first requests child termination; Azure-side work already accepted by the service may continue and is reported as such by azd.

## Authentication

The Connections center invokes each provider's supported login:

- `azd auth login`
- `az login`
- `Connect-AzAccount`
- `Connect-MgGraph`

The provider retains its credentials. The app reads only account/tenant/status output and warns when connected tenants do not align. Device-code login is an explicit fallback.

Future app-owned MSAL authentication can be introduced behind the same provider boundary, using WAM on Windows and a system-browser loopback redirect elsewhere.

## Persistence

SQLite is stored in the Tauri application-data directory. It contains project metadata, catalog sources and trust decisions, non-sensitive wizard drafts, and operation history. azd continues to own `.azure` environment files.

The template cache is content-addressed by template ID and commit. A cached Git checkout must resolve to the catalog commit before it is used.
