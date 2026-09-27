# Windows acceptance status

Current as of 26 September 2026. This is the operator-facing index for the next
Windows candidate. It reconciles the reusable checklist in
[Windows beta acceptance](windows-beta-acceptance.md) with the evidence in the
[20 September acceptance record](windows-acceptance-20260920.md) and the
[26 September native follow-up](windows-acceptance-20260926.md).

Status meanings:

- **Passed** — the named artifact/source and environment produced the retained
  evidence. It is not automatically transferable to another revision or image.
- **Blocked** — the exact missing input or maintainer decision is named.
- **Not run** — no qualifying evidence exists for the named candidate.
- **Failed** — a qualifying attempt found a defect or invalid artifact.

The current signed application candidate is version 0.1.1 from source
`cc67ad9378c053af6b97ab7df0bb5881c94ad5b1`, built by
[workflow 36272899991](https://github.com/nathanmcnulty/azd-gui/actions/runs/36272899991).
It contains the application and persistence changes through P6, the exact
Maester evidence-consumer adoption, and the optional-module readiness fix.
The workflow and a separate host download
check verified the signatures, checksums, version, installer payload, silent
install, and uninstall. Clean-profile authentication and affected visual and
management gates remain open. See the
[P7 release decision](p7-release-decision-20260920.md).
No tag or draft/public 0.1.1 release exists.

The matrix contains 26 beta gates: 17 Passed, 5 Not run, 3 Blocked, and 1 Failed.
The counts describe the retained evidence across explicitly named 0.1.1 sources;
they are not a release score. Each row states whether its evidence applies to the
current candidate or an older revision. The maintainer selected Windows 11 x64
for beta; Windows 10 coverage is outside this matrix.

## Current gate matrix

| Gate | Exact artifact/source and evidence | Status | Next owner action |
| --- | --- | --- | --- |
| Application validation and signing | Source `cc67ad9`; workflow `36272899991`; dependency audits, 136 frontend tests, Rust format/Clippy, 154 native tests with three ignored, release build, Trusted Signing, and installed-payload verification passed | **Passed** | Repeat only if application, dependencies, packaging, or workflow behavior changes. |
| Downloaded release artifacts | Current portable `3729991d9e58acaa3c97c2674d2c976fb0d2fd67b52680750836cab697ffcd52`; NSIS `067a0698d740bd4d1ab279e1958503ea114df7e79d4f6a3578d83c193fdb8405`; independent host download check found valid expected publisher, trusted timestamps, version, and checksums | **Passed** | Repeat against assets downloaded from an eventual tagged draft because that is a new evidence boundary. |
| Installed payload | Current installed executable `25b5d669abd74c6eb49252729101d18569d59f5dcb64fede9623cc96103ee43f`; runner and Sandbox standard-user checks verified version, signature, install, and startup | **Passed** | Repeat install/uninstall for an eventual tagged draft. |
| Clean standard-user Windows 11 installation and startup | Current source `cc67ad9`; Windows 11 Enterprise x64 Sandbox build `10.0.28000`; non-admin local `AzdAcceptance2`; Microsoft-signed WebView2 `154.0.4258.37`; signed NSIS installer exit 0, valid installed payload, visible GUI under the new user's profile | **Passed** | Repeat for an eventual tagged artifact. The signing runner independently verified the installer on Windows build `10.0.26100.0`; its native GUI was not observed there. |
| Native shell and admin surfaces | Signed source `a769ae6` rendered Home, Catalog, Projects, Connections, Tools, Settings, and the pinned starter wizard; dark appearance switched and the saved destination restored on reopen. Current source `cc67ad9` rendered Home, Catalog, Tools, and a bundled wizard under separate standard-user accounts | **Passed** | Repeat remaining affected surfaces and persistence behavior for the final signed/tagged source. |
| Missing prerequisites | Current signed source `cc67ad9` under fresh nonadmin `AzdAcceptance3` with disposable CLI path entries removed showed CLI tools 0/3 and all six Tools entries Missing; no install or elevation started | **Passed** | None unless tool detection or empty-state guidance changes. |
| Install prerequisites and refresh versions | Current signed source `cc67ad9` under a fresh standard user reported azd 1.34.1, Azure CLI 2.90.0, and PowerShell 7.6.6 as Ready; three absent PowerShell modules were Missing, and Home accurately showed CLI tools 3/3. The false 6/6 result from source `a769ae6` is retained in the follow-up record | **Passed** | Repeat against an eventual tagged artifact or if tool detection changes. |
| Clean-machine interactive authentication | Nora `nora@sharemylabs.com` exists in the expected tenant and has temporary Reader on the expected subscription; no retained clean-profile browser/WAM sign-in exists. Under the Sandbox secondary standard user, azd started but Windows denied its automatic Microsoft authorization-URL handoff; Edge itself launched directly under that user. This does not prove a defect in a normally logged-in profile | **Blocked** | Complete Nora's normal browser/WAM flow in a full standard-user session, or use azd's transient manual URL in Edge under the same disposable user. Never use device code or retain the authorization URL. |
| Wrong-tenant/target rejection after interactive sign-in | Automated and cached-host checks passed, including the tenant-scoped azd probe; no clean-profile live mismatch was exercised | **Blocked** | After Nora signs in, enter an unavailable subscription UUID as a read-only negative target and confirm Continue remains blocked. This tests an unavailable target, not access to a second real tenant. |
| Pinned source review and preparation in signed app | Signed app rendered the bundled catalog and restored drafts. Full pinned preparation ran in development source `94f60f7`, not signed 0.1.1 | **Not run** | After clean-VM tools/auth pass, prepare the named starter and retain the approval showing project, environment, template, and revision. |
| Preparation cancellation and partial-project recovery | Preview cancellation/retry passed in the host pilot. Cancellation during preparation was not exercised in signed 0.1.1 | **Not run** | Cancel one signed-candidate preparation and verify the partial project remains inspectable with explicit recovery/removal guidance. Do not delete it automatically. |
| Interactive-template terminal handoff | Deterministic tests cover the saved interactive route; no signed clean-VM handoff was observed | **Not run** | Open one reviewed interactive fixture/template and confirm only the validated external-terminal route is offered. Do not deploy it. |
| Non-sensitive persistence | Signed 0.1.1 retained theme, exact legacy draft/timestamp, and a new harmless draft across restart; SQLite integrity passed | **Passed** | None unless persistence schema or draft logic changes. |
| Sensitive-value non-persistence | Native allowlisting and tests exist; no signed clean-VM before/after inspection with a harmless designated sensitive fixture was retained | **Not run** | Use a non-secret marker in a field declared sensitive, close/reopen, and independently confirm it is absent from SQLite and UI. Never use a real secret for this test. |
| Missing-WebView2 fail-closed behavior | Predecessor `3971cb1`; its installed payload in a clean, network-disabled Windows Sandbox build `10.0.28000` stopped at the native “Could not find the WebView2 Runtime” bootstrap dialog with no simulated application state. This is predecessor evidence, not an exact-current-source native retest | **Passed** | Repeat against an eventual tagged artifact. The elevated `WDAGUtilityAccount` does not satisfy the separate standard-user gate. |
| Target review, permission preflight, preview invalidation | Development GUI source `94f60f7`; approved tenant/subscription; changing the resource group cleared approval and required a fresh preview | **Passed** | Treat as host-pilot evidence. Repeat only if target/preflight logic changes, or as part of the eventual solution pilot. |
| GUI deployment and resource view | Development GUI source `94f60f7`; reference `abc6e896620990e68360a63dd14efda66a3f75cd`; empty group `rg-gui-arm-pilot-20260919`; `azd up`, three checks, reopen, zero resources/endpoints | **Passed** | Do not repeat the empty-group pilot as solution proof. P7 must use the adopted solution and separate authorization. |
| Adopted Maester lifecycle | Exact Maester source `45f839bc`; authorized Minimal profile in `westus2`; Automation and Storage provisioned; eight approved Graph roles; completed runbook; archive and latest report written; four lifecycle checks passed; independent Azure and Graph cleanup clean | **Passed** | Treat missing-permission and `Forbidden` output as reduced Minimal-profile test coverage. See the [live pilot record](maester-minimal-pilot-20260921.md). |
| Retry/cancellation behavior | One preview cancelled and two previews succeeded before the successful host deployment; history persisted | **Passed** | Reuse regressions. Exercise a solution-specific safe retry/cancel path during P7. |
| Cleanup | GUI `azd down` succeeded; independent `az group exists` returned `false`; Azure CLI default subscription was unchanged | **Passed** | For P7, independently verify ARM and any directory/service objects owned by that solution. |
| Output/history secret review | Redaction and non-persistence tests passed, but no formal retained manual review of every signed-candidate output/history surface exists | **Not run** | During the next signed live path, inspect output/history for bearer values, JWTs, authorization headers, and sensitive environment values. |
| Corrected 0.1.0 → signed 0.1.1 upgrade | Corrected signed baseline `cc3d954` upgraded without elevation; exact draft/timestamp and settings retained | **Passed** | Preserve as corrected-baseline evidence; do not call it previous-public-version acceptance. |
| Previous public installer → candidate upgrade | Public `v0.1.0-alpha` outer installer is valid, but its installed executable is `HashMismatch` and was never launched | **Failed** | Maintainer accepted clean installation and excluded in-place upgrade from this historical package. Preserve the failure and follow the procedure below. |
| Clear local data | Signed 0.1.1 emptied five app-owned tables with SQLite integrity intact; test project file and Azure CLI directory marker retained identical hashes | **Passed** | Real credential caches were absent, so repeat credential-cache preservation only with an explicitly disposable signed-in profile. |
| Standard-user uninstall | Signed 0.1.1 uninstaller returned zero and installed executable was absent | **Passed** | Repeat for the eventual tagged draft. |
| Tag, draft release, and publication | No 0.1.1 tag/draft; interactive auth and several exact-candidate gates remain open; historical upgrade remains a documented failure with accepted clean-install handling | **Blocked** | Reconcile the remaining Windows 11 gates for the selected source and verify the eventual tagged artifact before deciding on publication. |

Private receipts and snapshots remain outside the repository under the existing
acceptance store and the run-specific private artifact store. They are evidence
inputs, not repository content. Do not commit credentials, raw CLI environments,
or private VM data.

### Evidence ledger

| Retained evidence | Establishes |
| --- | --- |
| Workflow `35670536219`; `installed-artifact-verification.json` | Exact `a769ae6` build/test/signing success plus runner-side signed installer payload, version, publisher, and timestamp verification |
| `host-artifact-verification.json`; `host-installed-verification.json` | Independently downloaded current artifact hashes/signatures plus host custom-directory install, installed-payload verification, and uninstall |
| `sandbox-install.json`; `sandbox-launch.json`; native computer-use capture | Predecessor `3971cb1` installed payload and signature in Windows Sandbox plus the observed missing-WebView2 native startup stop; Sandbox ran elevated and network-disabled |
| `azd-gui-a769ae6-clean-profile/prerequisites-local.json`; `medium-candidate-token.json` | On 21 September, a disposable Windows 11 Sandbox installed signed WebView2, ran local PowerShell 7.6.6 and Azure CLI 2.89.1, and launched source `a769ae6` with a filtered, medium-integrity token. The WDAG account was originally an administrator; this is prerequisite and process-token evidence, not a separate standard-user profile or completed GUI/authentication acceptance |
| `azd-gui-a769ae6-clean-profile/prepare.json`; `prerequisites-local.json`; `path-launch.json`; native computer-use observation | On 26 September, a fresh Sandbox verified exact signed source `a769ae6`, installed Microsoft-signed WebView2, and rendered the GUI with azd 1.34.1, Azure CLI 2.90.0, and PowerShell 7.6.6. The Tools page falsely marked absent Azure PowerShell, Microsoft Graph PowerShell, and ExchangeOnlineManagement modules Ready; Home showed 6/6. The GUI observation used the WDAG account, not a separate standard-user profile. |
| Workflow `36272899991`; `azd-gui-cc67ad9-clean-profile/artifact-verification.json`; `installed-artifact-verification.json`; `standard-install.json`; `missing-tools-launch.json`; `path-restored.json`; native computer-use observation | Corrected `cc67ad9` signed build and host download verification, Windows build 26100 runner installer check, and Windows build 28000 fresh standard-user NSIS installation/startup. The installed GUI marked absent modules Missing and showed CLI tools 3/3 when CLI tools were present, then 0/3 in a separate no-tools profile. No sign-in or Azure operation occurred. |
| `azd-gui-cc67ad9-clean-profile/browser-handoff-observation.json`; native computer-use observation | The signed app started azd Connect under a secondary Sandbox standard user, but Windows denied automatic opening of the authorization URL. Edge itself launched under that user. Azure CLI Connect started, but no clean-profile sign-in completed. The private receipt contains no authorization URL. |
| `final-artifact-verification.json` | Downloaded portable/installer hashes, version, expected publisher, and timestamps |
| `final-installed-verification.json` | Installed 0.1.1 hash, version, expected publisher, timestamp, and correspondence with the release receipt |
| `output/final/upgrade-data-verification.json` | Schema/integrity plus exact legacy draft, timestamp, theme, and navigation retention |
| `output/final/clear-database-verification.json` | App-owned database tables empty after native clear |
| `output/final/clear-preservation-after.json` | Harmless project and Azure CLI directory markers preserved; no real credentials tested |
| `output/final/final-uninstall.json` | Non-elevated uninstall exit and installed-executable absence |
| `gui-pilot-before.json`, `gui-pilot-provisioned.json`, `gui-pilot-after.json` | Approved target boundary, empty-group deployment/validation, and independent absence after cleanup |
| Private `azd-gui-maester-pilot-20260921` evidence store | Exact Maester source and values, East US 2 quota stop, West US 2 preview/deployment, eight Graph roles, RBAC and lock inventory, completed runbook and report blobs, template evidence, and independent Azure plus Graph cleanup |
| Native computer-use captures referenced by the acceptance record | Observed UI states; not a substitute for artifact or Azure receipts |

The ledger names files without publishing their private contents. A future operator
must match the receipt to the exact artifact/source before reusing a Passed result.

## Clean-machine authentication checklist

This is a signed-candidate acceptance check. It does not authorize deployment or
directory changes.

1. Start with a new standard-user Windows profile that has no Azure CLI, azd,
   PowerShell, or application credential cache. Do not simulate this by deleting
   a real user's cache.
2. Record the Windows build, signed app source/version, disposable identity
   `nora@sharemylabs.com`, expected tenant
   `847b5907-ca15-40f4-b171-eb18619dbfab`, and expected subscription
   `a80941e8-c2b9-4bc9-83ad-117cc40d0bea`. Use a valid but unavailable
   subscription UUID for the negative target check; do not probe another
   person's subscription.
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
7. Change to the controlled unavailable subscription target. Confirm the relevant
   connection becomes disconnected/unknown and Continue remains blocked. Return
   to the approved target and confirm a fresh check is required; a stale result
   must not unlock the wizard.
8. Close the app and retain the redacted observations. Either preserve the
   disposable snapshot for the remaining exact-candidate gates or discard it
   according to the acceptance owner. Do not sign out or delete caches on a
   non-disposable profile as part of this check.

Remaining live input: a human must complete Nora's broker/browser prompts in a
clean standard-user Windows 11 profile. The identity, expected tenant and
subscription, negative-target method, and exact private signed candidate are
already selected. No Azure resource group or application is required.

## Accepted clean-install handling for `v0.1.0-alpha`

The maintainer accepted this path on 21 September 2026. A warning was added to
the [historical prerelease notes](https://github.com/nathanmcnulty/azd-gui/releases/tag/v0.1.0-alpha)
on 26 September 2026. This does not remediate the already-published installer or
change its release assets. In-place upgrade from that installed package is unsupported.

1. Preserve the public assets and hashes as historical evidence. The prerelease
   notes must warn that the installer has a valid outer signature, but its
   installed application payload fails Authenticode and must not be launched.
   Do not silently replace an asset under the existing tag.
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
   component. Do not call this an in-place upgrade or change the failed matrix
   result.

The valid historical portable executable may remain useful as retained data-format
evidence, but it does not make the installed payload valid and is not a substitute
for testing the affected installed-version removal path.
