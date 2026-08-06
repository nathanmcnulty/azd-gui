# Azure Deployment Studio

A lightweight, cross-platform desktop experience for discovering trusted [Azure Developer CLI (`azd`)](https://learn.microsoft.com/azure/developer/azure-developer-cli/) templates, understanding their requirements, configuring environments, and deploying them without losing the power of the CLI.

The application uses Tauri 2, Svelte 5, TypeScript, and Rust. It does not bundle Chromium, store Azure tokens, or permit catalog manifests to run arbitrary commands.

## Current V1

- Compact VS Code-inspired shell with one persistent, collapsible navigation tree for Catalog, Projects, Connections, Tools, and Settings.
- System, light, and dark appearance preferences persisted with the navigation state.
- Dynamic deployment wizard with conditional manifest pages and preserved values.
- Bundled catalog containing pinned MyWorkID, Maester Automation, and Entra Health Monitoring templates.
- HTTPS/GitHub external catalog preview and explicit trust gate.
- Direct GitHub templates resolved to immutable commits before use.
- Local existing-project and copy-as-template workflows.
- Detection for azd, Azure CLI, PowerShell, Az PowerShell, and Microsoft Graph PowerShell.
- Native login orchestration for azd, Azure CLI, Az PowerShell, and Graph PowerShell, including device-code fallback and delegated Graph scope verification.
- Registered read-only Azure and Microsoft Graph permission checks with structured preflight results and required-failure blocking.
- Exact-commit template caching, `azd init`, environment value application, preflight preview, and `azd up --no-prompt`.
- Streaming operation output, cancellation, redaction, structured resource-change outcomes, safe retry actions, and local operation/project persistence.
- Project workspaces with environment management, lifecycle history, service endpoints, and validated Azure portal links from `azd show`.
- Explicit external-terminal fallback for interactive hooks, opened in a validated azd project with a copyable interactive `azd up` command.
- Explicit approval for vetted current-user module installs, including the provider, source, scope, destination, exact command, and official documentation.
- Read-only `azd tool list/check` integration for Azure CLI inventory and non-blocking update guidance, with copyable dry-run commands.
- Browser preview mode for UI development without Rust or Azure credentials.

## Development

Prerequisites:

- Node.js 22 or newer
- Rust stable
- Windows: Visual Studio Build Tools with the C++ workload
- macOS: Xcode command-line tools
- Linux: the [Tauri Linux prerequisites](https://v2.tauri.app/start/prerequisites/#linux)

```powershell
npm install
npm run dev
```

Run the native desktop application:

```powershell
npm run tauri dev
```

Build the portable application:

```powershell
npm run tauri build
```

The Windows build uses the installed WebView2 runtime and does not install a fixed runtime into the bundle.

## Signed Windows releases

The `Signed Windows release` workflow runs for `v*` tags and by manual dispatch. It uses GitHub OIDC—there is no client secret—to authenticate the `github-azd-gui` Entra application, signs the portable executable with the `kvpp-public-trust` Azure Artifact Signing profile, bundles it into an NSIS installer, and signs the installer.

The Azure IDs are stored as secrets on the dedicated `artifact-signing` GitHub Environment. The signing endpoint, account name, and certificate profile are non-secret workflow configuration. Security-sensitive Azure actions are pinned to immutable commits.

## Validation

```powershell
npm run ci
npm run format:native
npm run test:native
```

The CI matrix compiles and tests Windows, macOS, and Linux. Live Azure authentication and deployment checks remain manual because CI must not contain interactive user credentials.

## Catalog contracts

- [`schemas/azd-gui.catalog.schema.json`](schemas/azd-gui.catalog.schema.json) defines a catalog and requires every template to use an exact 40-character Git commit.
- [`schemas/azd-gui.schema.json`](schemas/azd-gui.schema.json) defines prerequisites, connection requirements, permission checks, conditional groups, and supported field types.
- [`catalog/azd-gui.catalog.json`](catalog/azd-gui.catalog.json) is compiled into the application and remains available offline.

An external catalog can be a direct HTTPS JSON URL or a GitHub repository containing `azd-gui.catalog.json` at its root. External content is previewed as untrusted and cannot enter the active catalog until the user explicitly accepts it.

Template manifest resolution is:

1. Catalog-selected authoritative manifest.
2. An embedded `azd-gui.json`.
3. Safe inference from `azure.yaml`.

Inference never creates executable commands. Unvetted interactive hooks are reported and can be run in an external terminal instead. The renderer cannot select a terminal executable or pass a shell command; Rust chooses from fixed platform providers after validating the project and environment.

## Security model

- The Svelte renderer has no shell capability.
- Rust accepts only registered executables and fixed operation/provider IDs.
- Process arguments are arrays, never renderer-supplied shell strings.
- Catalog repositories require HTTPS and pinned commits are reverified after checkout.
- Tokens, bearer headers, JWTs, and secret-like environment assignments are redacted from operation events.
- Sensitive wizard values are held only for the active deployment and are excluded from application drafts and logs. Rust independently whitelists typed draft fields, reapplies manifest sensitivity, and scrubs older drafts when they are loaded.
- Each Azure tool owns its own credential cache; the app reads status but does not capture tokens.
- The Tauri content security policy blocks remote scripts, objects, and frames.

See [docs/architecture.md](docs/architecture.md) for the subsystem and data-flow description.
