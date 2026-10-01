# Windows 11 acceptance preparation — 1 October 2026

This checkpoint prepares the existing App Isolation Workbench VM for the next
human acceptance attempt. It does not establish clean-profile authentication,
visual wizard acceptance, target rejection, or a release decision.

## Selected source and signing boundary

The first private candidate attempt for `6e1ef0a`,
[run 36912783063](https://github.com/nathanmcnulty/azd-gui/actions/runs/36912783063),
failed the npm dependency audit before Azure signing. Newly reported `devalue`
advisories affected 5.9.2. [PR 138](https://github.com/nathanmcnulty/azd-gui/pull/138)
updates only its lockfile entry to 5.9.4 in the existing Svelte dependency range.
Local full verify passed: 143 frontend and 163 native tests, three native
integration tests ignored, checks/build, formatting and strict Clippy. npm audit
reported zero vulnerabilities. Cargo audit passed against the retained local
database; live database fetching crashed the host auditor and requires the
workflow's fresh audit before signing.

The corrected application source is
`b018ed257917a7c5118c8bf1665c9820babda244`, selected for
[private signing run 36914798448](https://github.com/nathanmcnulty/azd-gui/actions/runs/36914798448).
The run succeeded: fresh dependency audits, 143 frontend and 163 native tests
(three ignored), native formatting/Clippy, release build, Trusted Signing,
signatures and runner install/payload/uninstall verification passed. Independent
host download checks confirmed exact checksums, version 0.1.1, the expected
Nathan McNulty publisher and trusted timestamps. No tag, draft, or public 0.1.1
release is authorized by this preparation.

| Artifact | SHA-256 |
| --- | --- |
| Portable executable | `cb274f8e67722275fa14316d81f825ca8cdd07c47556d7d7ecae8579fe5a405d` |
| NSIS installer | `5078015e945539342549a97611ee226d7c4f7d2af49e1ae393a25435c4e9d8cf` |
| Installed payload, verified on runner | `02647a93804b2d6ab3eca29130b16b617ac83508e397b599ebc01b0fcfe1b1e8` |
| Downloaded/transferred archive | `5a3f2e6118c1ab4e5edf88bde87f03e07f20342d3ddeae5795ef549fecf2be44` |

The portable and installed executable hashes legitimately differ because Tauri
patches the NSIS payload marker before signing. Never use the portable hash as
the installed-payload expectation. The new-profile helper pins the exact
installer and installed-payload hashes above before first launch.

The VM agent downloaded the private archive through a temporary artifact link.
Its archive SHA-256 matched the host download. VM-side checks then confirmed
both executables' exact hashes, valid expected publisher, trusted timestamps and
version 0.1.1 on Windows 11 Pro 25H2 build `26200.9457`. All transferred kit
metadata/helpers matched host file hashes. The first verification script stopped
before staging because its expected hash contained generator-added newlines;
the generator was corrected and the checks passed without changing the archive.
This was a preparation-script formatting defect, not an invalid signed artifact.

Private host evidence is under
`C:\Users\NathanMcNulty\.codex\artifacts\azd-gui-b018ed2-signed`:
`host-artifact-verification.json`, runner `installed-artifact-verification.json`,
`candidate-verification.json`, `transfer-result.log` and the VM verification
result. VM receipts `vm-artifact-verification.json` and `vm-kit-verification.json`
remain in the desktop kit. The VM receipt explicitly records no standard-user
installation and no authentication. Neither the new-profile app install helper
nor the account setup helper was executed during automated preparation.

## VM prerequisite preparation

The maintainer authorized preparation of the existing Workbench VM. Read-only
Azure inventory matched the requested machine to `aiw-clean-host-0921` in
`RG-AIW-CLEAN-HOST-20260921`. Its normal Windows 11 Pro 25H2 x64 RDP desktop was
unlocked; only the existing `aiwoperator` profile was established. This account
does not qualify as the new standard-user acceptance profile.

The VM agent installed the following fixed official packages machine-wide.
Before installation, each MSI had valid Microsoft Authenticode publisher and
timestamp signatures. Each installer returned zero without requesting a reboot;
subsequent executable version probes confirmed all three versions.

| Prerequisite | Version | Package SHA-256 |
| --- | --- | --- |
| PowerShell | 7.6.6 | `958838ff55091e1c8705d89efed0cc7e8245a3a6ef6c0ccfae20015227108ad8` |
| Azure CLI | 2.90.0 | `d5c1918eab32063219bea575e0d545149969c24751cb5f90ad2388b5df72222f` |
| Azure Developer CLI | 1.34.1 | `d158a676f73e35f3ea823d78647e0ca1e54f4291f935f8dfe321b3a07bb16f6a` |

WebView2 was already present. Existing Workbench files and other desktop kits
were preserved. No authentication, cache deletion, enrollment, role assignment,
Azure resource deployment, or reboot was performed.

## Desktop kit and human handoff

The kit lives on the VM's public desktop so a future local standard user can
read it: `C:\Users\Public\Desktop\AZD GUI Acceptance - b018ed2`.
Its `Signed app\START-HERE.txt` explains the selected source, verified install
helper, normal browser/WAM authentication and read-only positive/negative target
checks. Prerequisite receipts and fixed installers remain in the outer folder.

The human creates a new local standard-user profile with a private password and
signs into that Windows desktop normally. Nora's Azure identity is used only in
the app's normal browser/WAM prompts; it need not be the Windows account.
An optional local account setup helper runs only when the human opens an
elevated PowerShell on the operator desktop. It prompts privately for a new
Windows test-user password, creates `AzdAcceptanceNora` in Users and Remote
Desktop Users (never Administrators), and refuses to reset an existing account.
The human reconnects normally through RDP to initialize the new profile; no
test account or Windows password was created during automated preparation.
Existing users' authentication caches must not be copied or deleted. The install
helper rejects administrator-group membership, including a filtered admin token,
verifies both downloaded artifacts and the exact runner-recorded installed
payload before launching, and writes receipts under the new user's LocalAppData.
It refuses an existing installation directory and never removes one automatically.
An independent read-only review of the two helpers and human handoff found no
blocking issue; neither helper was executed as part of that review.

Expected tenant: `847b5907-ca15-40f4-b171-eb18619dbfab`.
Expected subscription: `a80941e8-c2b9-4bc9-83ad-117cc40d0bea`.
The maintainer's cached host session read-checked ARM assignments on 1 October:
Nora still has a direct Reader assignment at that subscription scope. This
control-plane observation does not prove token validity or Nora's own effective
target probe; refresh that probe after her clean-profile sign-in. No role changed.
The human completes `nora@sharemylabs.com` sign-in without sharing credentials,
codes, authorization URLs or tokens. Never use device-code authentication.
The first trial stops before preparation, preview, deployment or cleanup;
an unavailable random subscription UUID tests read-only rejection.

Private artifacts, download links, raw VM receipts and authentication state stay
outside Git. P7 remains **release no-go** until qualifying exact-candidate gates
are reconciled in the [gate matrix](windows-acceptance-status.md).
