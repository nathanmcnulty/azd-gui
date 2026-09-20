# Windows acceptance follow-up — 20 September 2026

This record separates native observations, signed packaging, and cloud deployment.
No public release was published during this pass.

## Named starter

The bundled **Azure deployment check** entry uses azd-reference commit
`abc6e896620990e68360a63dd14efda66a3f75cd`, `examples/deployment-check`.
It creates one dedicated empty resource group and an ARM subscription deployment
record, with three read-only postprovision checks. It creates no applications,
identities, or role assignments. Reference PR #33 passed 233 Windows tests and
hosted Windows/Linux validation. GUI PR #93 added the matching pinned manifest
and prepared version 0.1.1 for an actual installer version-increase test.

## Native findings and fixes

- In a new Windows Sandbox standard-user profile, the verified `cc3d954` portable
  executable stopped at the native missing-WebView2 error. No simulated operations
  were available. After installing the Microsoft-signed runtime, native navigation,
  catalog, wizard, and appearance controls rendered.
- The historical public `v0.1.0-alpha` installer has a valid outer signature, but
  its installed executable reports **HashMismatch**. It was never launched. Its
  independently verified public portable executable was used only to create and
  reopen a harmless local draft. This does not pass the previous-public-installer
  upgrade gate.
- The corrected `cc3d954` 0.1.0 installer installed without elevation and matched
  the signed payload receipt. Its dark appearance preference was retained as
  upgrade input. Legacy draft rows were preserved separately; this profile already
  had schema version 2, so it is not evidence of a schema-0 migration.
- Native testing exposed absent optional defaults serialized as null. These
  prevented draft saving and could display literal `null` in project configuration.
  PR #94 treats them as empty and keeps required-field checks effective. Its native
  development build saved and restored a MyWorkID draft after application restart;
  the corrected saved-choice notice also rendered at a readable width.

PR #94 passed 113 frontend tests, 131 native tests (three opt-in checks ignored),
Svelte checks, production build, formatting, and Clippy. npm reported no
vulnerabilities. RustSec passed with the documented exceptions against freshly
fetched advisory revision `d5c17953a895cf19e8d3ce66eaa42b6fcfe1fb16`.
The audit tool's initial network fetch crashed locally; fetching with Git and
rerunning `cargo audit --no-fetch --deny warnings` succeeded.

## Acceptance environment

The isolated acceptance VM is Windows 11 Enterprise, build `10.0.28000`, x64,
with a non-administrator `AzdAcceptance` profile. Network and clipboard sharing
are disabled. The Microsoft-signed WebView2 runtime is `153.0.4234.48`.
This record does not establish acceptance on every supported Windows build.

Immediately before final upgrade, the installed corrected baseline remained
version 0.1.0 with a valid expected-publisher signature and SHA-256
`9e177e4fc5eb7a4c9da162c2f12b80272451b53ad06b401992d2fff429048eb9`.

## Final native presentation check

PR #95 merged as `179b89e32b7d2990127f489bf25f4c230d164db6`. The rebuilt
native app showed the dedicated loading screen before real tools/accounts loaded.
A fresh pinned-starter workspace completed preview using the same approved target;
the next page displayed **Ready for azd up**, not deployment success. No second
provisioning operation was started. Signed workflow `35497506640` builds this
revision as version 0.1.1 for final artifact and upgrade acceptance.

## Signed 0.1.1 acceptance

[Signed workflow 35497506640](https://github.com/nathanmcnulty/azd-gui/actions/runs/35497506640)
passed for source `179b89e32b7d2990127f489bf25f4c230d164db6`. Downloaded artifacts
passed expected-version, SHA-256, Authenticode publisher, and timestamp checks.
The installed executable was verified before launch and matched the runner's
installed-payload receipt.

| Artifact | SHA-256 |
| --- | --- |
| Portable executable | `a8dfe371685f39340d871af0418cd019ecf1dae24d4ebca3a11b091f39e3e2e0` |
| NSIS installer | `a5e906f69bff52f299b7d5f230cffa965ac6dee46fafd73a10d5a4b49c5c9820` |
| Installed executable | `34d92922abf4734206c2af75ef059cf5a8a9a0bfb0f8e25fff2fc2da10ed7c24` |

The same standard-user installation directory upgraded from the verified 0.1.0
baseline to 0.1.1 without elevation. The native app launched successfully and
retained dark appearance. After process exit, independent SQLite comparison
confirmed schema 2, integrity, the exact legacy draft values/timestamp, and saved
appearance/navigation settings. This proves retention, not schema-0 migration
or execution of the invalid historical installed payload.

Native Home, Catalog, Wizard, Connections, Tools, and Settings rendered. A new
non-sensitive draft saved and restored after a full process restart, with the
readable saved-choice notice. Missing prerequisites and absent accounts remained
visible; no tool installation or authentication was started in this offline VM.

The native-confirmed **Clear local data** flow reset appearance and reloaded Home.
Independent inspection found zero rows in projects, operation history, catalog
sources, wizard drafts, and app settings, with SQLite integrity intact. A harmless
project file and marker in the Azure CLI directory retained identical hashes.
No real credentials were present, so this is not a credential-cache live test.
Standard-user uninstall returned zero and the installed executable was absent.

## Authenticated native GUI pilot

The host native GUI recognized installed prerequisites and cached CLI accounts,
and showed the exact starter revision and approved Azure target. Preparation
stopped before any cloud mutation because `azd auth status` omits tenant metadata
and the GUI treated the missing metadata as a mismatch. The tenant-scoped
read-only `azd auth login --check-status --tenant-id ... --output json --no-prompt`
returned `status: success`. The native integration now verifies user credentials
for the selected tenant without returning tokens, and ignores late checks for
earlier wizard sessions or targets. The live GUI subsequently passed account
alignment, subscription permission checks, and pinned project preparation.
Preview cancellation reported cancellation and kept deployment blocked; the
prepared project remained available for retry. Retry succeeded; editing the Azure
target cleared approval and required a new preview. The GUI then ran `azd up`
successfully against the authorized empty-resource-group target. All three
validation checks passed (zero failed, warning, or skipped checks). Independent
Azure queries confirmed the approved group in `westus2`, provisioning succeeded,
and zero resources inside the group.

After closing and reopening the app, the registered project retained its source
revision, validation report, and operation history. The resources view correctly
reported no services or endpoints for this infrastructure-only starter. GUI
`azd down` succeeded; independent `az group exists` returned false afterward.
The Azure CLI default subscription matched its pre-pilot value. Local project
files and historical validation evidence remain; they are not current resource
existence claims. This was the native development build at `94f60f7` with cached
host authentication, not final signed or clean-VM authentication acceptance.

The pilot also exposed preview success incorrectly marking the wizard deployment
complete before `azd up` began. Deployment presentation now follows only the
wizard's matching deployment operation, including asynchronous startup. Cancellation targets that exact operation, and completion received before command startup returns does not block retry. Independent review confirmed both event-race corrections.

An additional startup fix hides simulated preview data while native bootstrap is
pending and catches operation-listener initialization failures. The integrated
tenant fix passed 114 frontend and 133 native tests; the subsequent startup-only
and deployment-status changes passed all 118 frontend tests and Svelte checks.

Signed runs `35495517974` and `35495878388` were intentionally canceled after
native acceptance exposed defects, before promoting either candidate. Final
signed upgrade/uninstall checks passed on the revision containing the fixes.
Clean-VM interactive authentication still needs the selected disposable account
and user completion of any broker/browser prompt. Host cached-session checks do
not satisfy that gate. Clean-VM interactive authentication remains open. The previous-public-installer upgrade path remains unverified because its installed payload has an invalid signature; corrected-baseline upgrade and legacy portable data retention passed separately. No public release has been promoted.

Private local evidence is retained under `E:\azd-gui-acceptance-remaining`,
including artifact signatures/hashes, Sandbox installation results, database
snapshots, and validation logs. UI observations are recorded in the task's native
computer-use captures. Do not commit credentials, CLI environments, or raw private
acceptance evidence.
