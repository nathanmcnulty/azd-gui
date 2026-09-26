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
