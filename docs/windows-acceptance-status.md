# Windows acceptance status

Current as of 20 September 2026. This is the operator-facing index for the next
Windows candidate. It reconciles the reusable checklist in
[Windows beta acceptance](windows-beta-acceptance.md) with the evidence in the
[20 September acceptance record](windows-acceptance-20260920.md).

Status meanings:

- **Passed** — the named artifact/source and environment produced the retained
  evidence. It is not automatically transferable to another revision or image.
- **Blocked** — the exact missing input or maintainer decision is named.
- **Not run** — no qualifying evidence exists for the named candidate.
- **Failed** — a qualifying attempt found a defect or invalid artifact.

The current signed application candidate is version 0.1.1 from source
`3971cb1f685eae93c48c0fa37972b226d29e286f`, built by
[workflow 35655808067](https://github.com/nathanmcnulty/azd-gui/actions/runs/35655808067).
It contains the application and persistence changes through P6 plus the exact
Maester evidence-consumer adoption. The workflow and a separate host download
check verified the signatures, checksums, version, installer payload, silent
install, and uninstall. Clean-profile authentication and affected visual and
management gates remain open. See the
[P7 release decision](p7-release-decision-20260920.md).
No tag or draft/public 0.1.1 release exists.

The matrix contains 26 gates: 14 Passed, 8 Not run, 3 Blocked, and 1 Failed.
The counts describe the retained evidence across explicitly named 0.1.1 sources;
they are not a release score. Each row states whether its evidence applies to the
current candidate or an older revision.

## Current gate matrix

| Gate | Exact artifact/source and evidence | Status | Next owner action |
| --- | --- | --- | --- |
| Application validation and signing | Source `3971cb1`; workflow `35655808067`; dependency audits, 135 frontend tests, Rust format/Clippy, 153 native tests, release build, Trusted Signing, and installed-payload verification passed | **Passed** | Repeat only if application, dependencies, packaging, or workflow behavior changes. |
| Downloaded release artifacts | Current portable `8770f927399b1dc74492961831c2f0380607fc09e7a9e5154215a060aa2a870c`; NSIS `366202e23131ca6051ac770ae8f83ba24d5a90180d42b037c58efd05bac72039`; host download check found valid expected publisher, trusted timestamps, version, and checksums | **Passed** | Repeat against assets downloaded from an eventual tagged draft because that is a new evidence boundary. |
| Installed payload | Current installed executable `f1df7ff009eaf0ef5face5ae546d36615b3e7c225905c463d2c6b12823e9e9d8`; runner and host checks verified version, signature, publisher, timestamp, install, and uninstall | **Passed** | Repeat on the clean acceptance image and for an eventual tagged draft. |
| Clean standard-user Windows 11 installation and startup | Older source `179b89e`; Windows 11 Enterprise x64 build `10.0.28000`; non-admin `AzdAcceptance`; WebView2 `153.0.4234.48`; signed 0.1.1 | **Passed** | Repeat startup and affected surfaces for current source `3971cb1`; record a supported production Windows build when selecting the beta release image. |
| Windows 10 coverage | No retained Windows 10 native acceptance for 0.1.1 | **Not run** | Release owner decides whether Windows 11 evidence is sufficient for the beta claim or runs the affected gates on a supported Windows 10 x64 image. |
| Native shell and admin surfaces | Older source `179b89e`; signed 0.1.1 Home, Catalog, Wizard, Connections, Tools, and Settings; dark theme; new draft save/reopen | **Passed** | Repeat surfaces affected after `179b89e` against current source `3971cb1`, then repeat the full visual pass for the tagged draft. |
| Missing prerequisites | Signed 0.1.1 clean profile reported six missing tools and no connected accounts without installation, elevation, or simulated success | **Passed** | None unless tool detection or empty-state guidance changes. |
| Install prerequisites and refresh versions | Installed tools refreshed correctly in the host development build; they were not installed in the signed clean VM | **Not run** | Clean-VM operator installs supported prerequisites from official sources, restarts signed candidate, and records detected versions. |
| Clean-machine interactive authentication | Signed clean VM remained offline/disconnected; cached host authentication is separate evidence | **Blocked** | User supplies the disposable identity/tenant and completes normal browser/WAM prompts. Follow the checklist below; never use device code. |
| Wrong-tenant/target rejection after interactive sign-in | Automated and cached-host checks passed, including the tenant-scoped azd probe; no clean-profile live mismatch was exercised | **Blocked** | The authentication operator supplies a controlled second tenant/target and confirms Continue remains blocked. No resource mutation is needed. |
| Pinned source review and preparation in signed app | Signed app rendered the bundled catalog and restored drafts. Full pinned preparation ran in development source `94f60f7`, not signed 0.1.1 | **Not run** | After clean-VM tools/auth pass, prepare the named starter and retain the approval showing project, environment, template, and revision. |
| Preparation cancellation and partial-project recovery | Preview cancellation/retry passed in the host pilot. Cancellation during preparation was not exercised in signed 0.1.1 | **Not run** | Cancel one signed-candidate preparation and verify the partial project remains inspectable with explicit recovery/removal guidance. Do not delete it automatically. |
| Interactive-template terminal handoff | Deterministic tests cover the saved interactive route; no signed clean-VM handoff was observed | **Not run** | Open one reviewed interactive fixture/template and confirm only the validated external-terminal route is offered. Do not deploy it. |
| Non-sensitive persistence | Signed 0.1.1 retained theme, exact legacy draft/timestamp, and a new harmless draft across restart; SQLite integrity passed | **Passed** | None unless persistence schema or draft logic changes. |
| Sensitive-value non-persistence | Native allowlisting and tests exist; no signed clean-VM before/after inspection with a harmless designated sensitive fixture was retained | **Not run** | Use a non-secret marker in a field declared sensitive, close/reopen, and independently confirm it is absent from SQLite and UI. Never use a real secret for this test. |
| Missing-WebView2 fail-closed behavior | Verified portable 0.1.0 baseline stopped at the native runtime bootstrap error with no simulated operations; final 0.1.1 was not run without WebView2 | **Not run** | On a disposable snapshot, repeat with the exact tagged portable candidate, then restore the snapshot. |
| Target review, permission preflight, preview invalidation | Development GUI source `94f60f7`; approved tenant/subscription; changing the resource group cleared approval and required a fresh preview | **Passed** | Treat as host-pilot evidence. Repeat only if target/preflight logic changes, or as part of the eventual solution pilot. |
| GUI deployment and resource view | Development GUI source `94f60f7`; reference `abc6e896620990e68360a63dd14efda66a3f75cd`; empty group `rg-gui-arm-pilot-20260919`; `azd up`, three checks, reopen, zero resources/endpoints | **Passed** | Do not repeat the empty-group pilot as solution proof. P7 must use the adopted solution and separate authorization. |
| Retry/cancellation behavior | One preview cancelled and two previews succeeded before the successful host deployment; history persisted | **Passed** | Reuse regressions. Exercise a solution-specific safe retry/cancel path during P7. |
| Cleanup | GUI `azd down` succeeded; independent `az group exists` returned `false`; Azure CLI default subscription was unchanged | **Passed** | For P7, independently verify ARM and any directory/service objects owned by that solution. |
| Output/history secret review | Redaction and non-persistence tests passed, but no formal retained manual review of every signed-candidate output/history surface exists | **Not run** | During the next signed live path, inspect output/history for bearer values, JWTs, authorization headers, and sensitive environment values. |
| Corrected 0.1.0 → signed 0.1.1 upgrade | Corrected signed baseline `cc3d954` upgraded without elevation; exact draft/timestamp and settings retained | **Passed** | Preserve as corrected-baseline evidence; do not call it previous-public-version acceptance. |
| Previous public installer → candidate upgrade | Public `v0.1.0-alpha` outer installer is valid, but its installed executable is `HashMismatch` and was never launched | **Failed** | Maintainer chooses the clean-install migration proposal below or another reviewed remediation. Never waive or relabel this failure. |
| Clear local data | Signed 0.1.1 emptied five app-owned tables with SQLite integrity intact; test project file and Azure CLI directory marker retained identical hashes | **Passed** | Real credential caches were absent, so repeat credential-cache preservation only with an explicitly disposable signed-in profile. |
| Standard-user uninstall | Signed 0.1.1 uninstaller returned zero and installed executable was absent | **Passed** | Repeat for the eventual tagged draft. |
| Tag, draft release, and publication | No 0.1.1 tag/draft; interactive auth and several exact-candidate gates remain open; historical upgrade failed | **Blocked** | User authorizes tag/signing/publication only after the matrix is reconciled for the selected release revision and accepts or resolves the historical migration limitation. |

Private receipts and snapshots remain outside the repository under the existing
acceptance store and the run-specific private artifact store. They are evidence
inputs, not repository content. Do not commit credentials, raw CLI environments,
or private VM data.

### Evidence ledger

| Retained evidence | Establishes |
| --- | --- |
| Workflow `35655808067`; `installed-artifact-verification.json` | Exact `3971cb1` build/test/signing success plus runner-side signed installer payload, version, publisher, and timestamp verification |
| `downloaded-artifact-verification.json`; `host-installed-verification.json` | Independently downloaded current artifact hashes/signatures plus host custom-directory install, installed-payload verification, and uninstall |
| `final-artifact-verification.json` | Downloaded portable/installer hashes, version, expected publisher, and timestamps |
| `final-installed-verification.json` | Installed 0.1.1 hash, version, expected publisher, timestamp, and correspondence with the release receipt |
| `output/final/upgrade-data-verification.json` | Schema/integrity plus exact legacy draft, timestamp, theme, and navigation retention |
| `output/final/clear-database-verification.json` | App-owned database tables empty after native clear |
| `output/final/clear-preservation-after.json` | Harmless project and Azure CLI directory markers preserved; no real credentials tested |
| `output/final/final-uninstall.json` | Non-elevated uninstall exit and installed-executable absence |
| `gui-pilot-before.json`, `gui-pilot-provisioned.json`, `gui-pilot-after.json` | Approved target boundary, empty-group deployment/validation, and independent absence after cleanup |
| Native computer-use captures referenced by the acceptance record | Observed UI states; not a substitute for artifact or Azure receipts |

The ledger names files without publishing their private contents. A future operator
must match the receipt to the exact artifact/source before reusing a Passed result.

## Clean-machine authentication checklist

This is a signed-candidate acceptance check. It does not authorize deployment or
directory changes.

1. Start with a new standard-user Windows profile that has no Azure CLI, azd,
   PowerShell, or application credential cache. Do not simulate this by deleting
   a real user's cache.
2. Record the Windows build, signed app source/version, disposable identity,
   expected tenant, and expected subscription. The user must also identify a
   controlled mismatched tenant/target for the negative check.
3. Launch the installed signed app. Confirm Connections shows no active account
   and neither the UI nor output contains tokens.
4. Use the app's **Connect** action. Allow azd/Azure CLI to use their normal
   broker or system-browser flow; the human completes sign-in and consent. Do not
   use, suggest, or fall back to device-code authentication.
5. Refresh Connections. Record only provider, account label, tenant,
   subscription, expiry/status, and the detected executable/version. Never copy
   token caches or raw authentication responses into evidence.
6. Open the Azure deployment check starter. Select the expected tenant and
   subscription. Confirm both required connections are shown as connected for
   that target and the wizard can reach target review. Stop before preparation;
   sign-in proof does not require project initialization or cloud mutation.
7. Change to the controlled mismatched tenant/target. Confirm the relevant
   connection becomes disconnected/unknown and Continue remains blocked. Return
   to the approved target and confirm a fresh check is required; a stale result
   must not unlock the wizard.
8. Close the app and retain the redacted observations. Either preserve the
   disposable snapshot for the remaining exact-candidate gates or discard it
   according to the acceptance owner. Do not sign out or delete caches on a
   non-disposable profile as part of this check.

Inputs still required: disposable identity, expected tenant/subscription,
controlled mismatched target, a human to complete broker/browser prompts, and the
exact signed candidate. No Azure resource group or application is required.

## Proposed clean-install migration for `v0.1.0-alpha`

This proposal does not remediate the already-published installer and does not
authorize editing or deleting its release assets.

1. Preserve the public assets and hashes as historical evidence. Add a prominent
   warning to the prerelease notes only after maintainer approval: the installer
   has a valid outer signature, but its installed application payload fails
   Authenticode and must not be launched. Do not silently replace an asset under
   the existing tag.
2. On an affected machine, close the old app without launching it. Record the
   installed path, file hash, version, and signature result. Back up the app-owned
   SQLite files (`azd-gui.sqlite`, `-wal`, and `-shm` together while the process is
   stopped) and separately inventory user project directories. Exclude CLI token
   caches and secrets from the backup.
3. Before using the registered old uninstaller, independently verify the
   uninstaller's signature and exact path. This has not yet been established. If
   it is invalid, missing, or unexpected, stop; do not execute it. Prefer a fresh
   disposable Windows profile/VM, or prepare a separately reviewed, narrowly
   scoped cleanup procedure with recovery copies. Do not ask the invalid installed
   application to remove itself.
4. After verified removal, confirm the old executable and install directory are
   absent. Install the new candidate into an empty directory as a standard user.
   Verify its installer, installed executable, signer, timestamp, version, and
   installed-payload receipt before first launch.
5. Keep the backup read-only. Restore supported non-sensitive state only after the
   new candidate has launched cleanly and a copy of the database passes integrity
   and migration checks. Preserve project folders independently. Never restore an
   executable, credential cache, or unknown file from the old install directory.
6. Record one of three outcomes: clean profile/no state restored; verified
   migration succeeded; or migration stopped before executing an unverified
   component. A maintainer must decide whether this documented clean-install path
   is acceptable for beta publication or whether a tested cleanup tool is required.

The valid historical portable executable may remain useful as retained data-format
evidence, but it does not make the installed payload valid and is not a substitute
for testing the affected installed-version removal path.
