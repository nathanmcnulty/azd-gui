# Windows beta acceptance

A tagged build stages signed artifacts in a draft GitHub Release. The draft must remain private until this checklist passes on a clean Windows 10 or Windows 11 x64 virtual machine. Do not substitute a developer workstation or a profile that already contains Azure tool state.

## Before tagging

- Start from a clean worktree and run `npm ci`, `npm run verify`, and `npm run audit:dependencies` with Node.js 22, Rust stable, and the Windows C++ build tools.
- Confirm `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` contain the same release version.
- Review every lifecycle hook in each bundled pinned template.
- Record the tester, intended tag, source commit, clean-VM image/version, test subscription, and cleanup owner.
- Create the `v<version>` tag only after the candidate commit and hosted signing run are intentionally authorized.

## Verify the draft artifacts

Before downloading the release, copy the expected Authenticode subject from the `kvpp-public-trust` profile's independently accessed Azure configuration. Download `azd-gui.exe`, the NSIS installer, and `SHA256SUMS.txt` from the draft release into an otherwise empty directory. Obtain `scripts/verify-release-artifacts.ps1` from the exact tagged source, then run:

```powershell
pwsh -File .\scripts\verify-release-artifacts.ps1 `
  -ReleaseDirectory C:\beta-candidate `
  -ExpectedVersion 0.1.0 `
  -ExpectedSignerSubject $expectedSignerSubject
```

Retain the generated `artifact-verification.json` in the private release record. The verifier fails unless the directory contains the expected portable executable and one x64 installer with no unlisted executables, both hashes match the published checksum file, both Authenticode signatures and timestamps are trusted, both signer subjects match the independently obtained profile identity, and the portable executable reports the requested version.

## Clean-VM smoke test

Begin from a new snapshot with no repository checkout, Node.js, Rust, Azure credentials, azd project, or application data.

- Confirm Windows is fully patched and the WebView2 Runtime is installed. The beta uses the system runtime and does not download one during setup.
- Install the signed NSIS package without elevation. Confirm the installed app launches with no lingering console window.
- Confirm the navigation, theme, catalog, empty Projects page, Connections page, Tools page, and Settings page render correctly.
- Confirm missing tools are reported without attempting elevation, installation, or a browser-preview fallback.
- Install supported azd/Azure prerequisites using their official installers, restart the app, and confirm tool versions refresh.
- Sign in interactively only with the disposable test identity. Confirm connection state and tenant/subscription context without exposing tokens in output.
- Prepare a bundled template. Confirm the pinned commit and template-execution approval are shown before any hook or lifecycle operation can run.
- Cancel preparation once and confirm the partial project remains visible with inspect, resume, and remove guidance.
- Confirm a manifest-declared interactive template opens the validated external-terminal handoff.
- Close and reopen the app. Confirm non-sensitive navigation/project state persists and sensitive wizard values do not.

On a disposable snapshot where WebView2 can be removed safely, also launch the portable executable without the runtime. The app must stop at the Windows/Tauri runtime bootstrap error; it must not expose simulated browser-preview operations. Restore the snapshot afterward rather than altering the acceptance VM in place.

## Authorized deployment smoke test

Do not perform this section without explicit authorization for the tenant, subscription, template, and cleanup plan.

- Use a disposable Azure environment and resource group.
- Run permission preflight and infrastructure preview. Treat preparation and preview as potentially mutating.
- Change one environment value and confirm the prior preview is invalidated.
- Deploy once, verify returned endpoints and `azd show --output json` state, then exercise one safe retry or cancellation path.
- Run the template's documented cleanup path and independently verify Azure resources and directory assignments were removed. Record every intentional remainder.
- Confirm output and persisted history contain no bearer values, JWTs, sensitive environment values, or authorization headers.

## Upgrade, uninstall, and publish

- From a fresh snapshot, install the previous public signed version, create only non-sensitive local state, then install the candidate. Confirm the app starts and retained state remains usable.
- Use **Settings → Clear local data** and confirm project references, history, drafts, catalog trust, preferences, and cached templates are removed while project folders and external CLI credentials remain. Then uninstall the candidate, confirm installed binaries are removed, and discard the VM or manually remove the intentionally retained test folders and credentials.
- Attach the artifact evidence and completed checklist to the private release record. Document failures; never work around a failed signature, checksum, version, bootstrap, deployment, cleanup, upgrade, or uninstall check.
- Publish the draft only after every required item passes: `gh release edit v<version> --draft=false`.
- Preserve the VM snapshot and evidence until the beta has been available long enough to investigate installation reports.
