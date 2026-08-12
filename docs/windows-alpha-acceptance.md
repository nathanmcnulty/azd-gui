# Windows alpha acceptance

Use this checklist for a local alpha candidate. It intentionally does not dispatch GitHub Actions or run hosted macOS/Linux builds.

## Build gate

- Use Node.js 22 from `.nvmrc`, Rust stable, and the Windows C++ build tools.
- Start from a clean worktree and run `npm ci`.
- Run `npm run verify`.
- Run `npm run tauri build` and retain the portable executable and NSIS installer paths.
- Record SHA-256 hashes for the produced artifacts.
- Confirm the portable executable is a Windows GUI binary and launches without a parent console window.
- Check Authenticode status. `NotSigned` is expected for a local alpha build; only the tagged release path signs artifacts.

## Disposable-profile smoke test

Use Windows Sandbox, a disposable local user, or another fresh profile. Do not reuse a profile containing production-only azd or Azure defaults.

- Launch the portable app and confirm no console window remains open.
- Verify the navigation collapse, output resize/maximize controls, theme persistence, and catalog/project pages.
- Verify the app reports missing tools without attempting elevation or installation.
- Sign in to each needed provider interactively, then confirm the connection tile refreshes and shows the selected tenant/subscription where supported.
- Select a different test subscription and confirm the guided flow requests realignment without changing the global `az account show` default.
- Confirm output strips ANSI escape sequences and does not display tokens or sensitive environment values.
- Confirm a project lifecycle action is disabled until the session template-execution approval is checked.
- Confirm preview completion is labeled as possibly modified and never claims Azure was unchanged.
- Confirm a manifest-declared interactive template routes deployment to the external-terminal handoff.

## Live deployment gate

Do not perform this section without explicit authorization for the tenant, subscription, template, and cleanup plan.

- Use one disposable environment at a time.
- Review the pinned template commit and all lifecycle hooks before approval.
- Capture the selected tenant, subscription, region, resource group, and manifest inputs.
- Treat project initialization and preview as potentially mutating.
- After deployment, verify returned endpoints and structured `azd show --output json` state.
- Run the template's documented cleanup path, then independently verify Azure resources and directory assignments were removed or document every intentional remainder.

## Release handoff

- Keep the validated commit unchanged after acceptance.
- Do not create a `v*` tag until the signed Windows release is intentionally authorized.
- A tag starts the only automatic hosted workflow; ordinary branches and pull requests must consume zero Actions minutes.
