# Windows 11 acceptance follow-up — 26 September 2026

This is a source-level native observation of the private signed 0.1.1 candidate
from commit `a769ae68b7db8c4b118374026412bab54dc73796`. It is not a tagged release
or a completed standard-user acceptance pass.

## Disposable environment and artifact

The Windows Sandbox used Windows 11 Enterprise x64 build `10.0.28000` and a
fresh WDAGUtilityAccount profile. The source executable had SHA-256
`06dc34656e9be603e00e8f8744053ec0b9ae1a265fd9fe1a66be7e30c11cc861`
and valid expected-publisher Authenticode. A Microsoft-signed WebView2
bootstrapper completed with exit code 0; the runtime registered as
`154.0.4258.37`. Local copies of azd 1.34.1, Azure CLI 2.90.0, and PowerShell
7.6.6 ran. The application received those local tool paths in its launch
environment and rendered without the previous missing-WebView2 dialog.

A separate process was created with a non-administrator medium-integrity token,
but the visible GUI observations below were made in the WDAG account's normal
session. They do not prove behavior for a separate standard-user profile.

## Native observations

- Home, Catalog, Projects, Connections, Tools, and Settings rendered. The bundled
  catalog showed four pinned templates. Settings switched to dark appearance.
- The **Azure deployment check** wizard displayed the reviewed repository,
  commit `abc6e896620990e68360a63dd14efda66a3f75cd`, template path, and
  source-integrity boundary. Its prerequisite page correctly found azd 1.34.1,
  Azure CLI 2.90.0, and PowerShell 7.6.6. With no cached accounts, its account
  page showed azd and Azure CLI disconnected and blocked Continue.
- After closing and reopening the wizard, the non-sensitive destination
  `C:\Acceptance\pilot-project` reappeared with a saved-choices notice.
- The general Tools page incorrectly reported **Ready** for Azure PowerShell,
  Microsoft Graph PowerShell, and ExchangeOnlineManagement even though each
  installed-version field said **Not found**. Home therefore overstated local
  readiness as **6/6**. The native probe treated a successful empty module
  query as an installed version. [PR #118](https://github.com/nathanmcnulty/azd-gui/pull/118)
  fixed the probe and made Home count its three displayed CLI tools. The signed
  candidate remains a failed prerequisite-display result until the corrected
  signed build is observed.

No account sign-in, project preparation, Azure resource mutation, deployment,
or cleanup occurred in this Sandbox pass. The browser/WAM sign-in still needs a
human operator with the designated disposable account. The negative subscription
target and any post-sign-in wizard observations remain untested.

Private receipts are under
`C:\Users\NathanMcNulty\.codex\artifacts\azd-gui-a769ae6-clean-profile`:
`prepare.json`, `prerequisites-local.json`, `medium-candidate-token.json`, and
`path-launch.json`. The UI findings were observed through native computer use in
this pass; they are not inferred from those setup receipts.

## Corrected signed candidate and standard-user install

[Private workflow 36272899991](https://github.com/nathanmcnulty/azd-gui/actions/runs/36272899991)
built and signed source `cc67ad9378c053af6b97ab7df0bb5881c94ad5b1`.
It passed dependency audits, 136 frontend tests, Rust formatting and Clippy,
154 native tests with three explicitly ignored, release build, Azure signing,
and installed-payload verification. Its Windows runner used build `10.0.26100.0`.
Independent host verification of the downloaded private artifact found valid
expected-publisher signatures, timestamps, version 0.1.1, and checksums:

| Artifact | SHA-256 |
| --- | --- |
| Portable executable | `3729991d9e58acaa3c97c2674d2c976fb0d2fd67b52680750836cab697ffcd52` |
| NSIS installer | `067a0698d740bd4d1ab279e1958503ea114df7e79d4f6a3578d83c193fdb8405` |
| Installed executable | `25b5d669abd74c6eb49252729101d18569d59f5dcb64fede9623cc96103ee43f` |

In the same disposable Windows 11 Sandbox, a newly created local account
`AzdAcceptance2` belonged to Users and not Administrators. Under that account,
the exact signed NSIS installer completed silently with exit code 0 in a new
directory. The installed executable matched the workflow's signed-payload hash
and had valid Authenticode. Its visible process was owned by `AzdAcceptance2`;
WebView2 used that account's separate local profile. This establishes a real
standard-user installation and startup on Sandbox build `10.0.28000`, not just
the earlier filtered-token launch.

The installed GUI detected azd 1.34.1, Azure CLI 2.90.0, and PowerShell 7.6.6.
The Tools page now marked the three absent PowerShell modules **Missing** and
offered installation review only where supported. Home showed **CLI tools 3/3**
for the three tools it lists. The clean profile's Connections page had no active
account. At this stage no authentication or Azure mutation occurred, and the new candidate
has no tag or draft release.

The downloaded binaries and receipts are retained privately under
`C:\Users\NathanMcNulty\.codex\artifacts\azd-gui-cc67ad9-clean-profile`.
The native GUI findings were observed through computer use, separately from
the installer and process receipts.

For the missing-prerequisite case, a third fresh nonadmin account
`AzdAcceptance3` launched the same installed executable with the disposable
CLI path entries removed. Its visible Home page showed **CLI tools 0/3**, and
Tools marked all three CLI tools and all three optional modules **Missing**.
The recorded process was owned by that account and remained running. No tool
installation was started. The tool path was restored after observation; the
private `missing-tools-launch.json` and `path-restored.json` receipts preserve
the setup boundary. This was a second clean profile in the same Sandbox, not a
second Windows installation.

## Browser handoff in the disposable profile

The corrected signed app's Azure Developer CLI **Connect** action started azd
1.34.1 under `AzdAcceptance2`. Windows then showed an access-denied dialog for
the Microsoft authorization URL. azd reported that it could not open the next
URL and offered a manual browser URL in the app's transient output. No account
connected and no credentials were entered. Microsoft Edge launched successfully
as the same standard user when started by executable path, so the browser is
present. Azure CLI **Connect** also started, but no completed sign-in was
observed. The attempt has not established whether the URL handoff failure is
caused by this secondary user's profile or by the app/CLI path; Windows Sandbox
normally logs into `WDAGUtilityAccount`, while this test launches the other
user's GUI on that desktop. Do not infer that an ordinary logged-in standard
user would encounter the same error.

Continue acceptance through a normal browser/WAM flow in a full standard-user
session, or use azd's displayed manual authorization URL in a browser running
as the same disposable user. A human must still complete Nora's sign-in. Never
use device-code authentication. Do not retain the one-time authorization URL
or token/cache material in repository or evidence files.
