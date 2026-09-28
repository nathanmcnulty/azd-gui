# Windows 11 VM readiness, 28 September 2026

This is a read-only observation of the open RDP desktop, not a signed-app
acceptance result. No application, prerequisite, account, or authentication
change was made in the VM during this inspection.

| Item | Observation |
| --- | --- |
| VM | `aiwcleanhost`, reached through the open RDP session |
| OS | Windows 11 Pro 25H2, x64, build `26200.9457` |
| Active Windows profile | Local `aiwoperator`; an elevated PowerShell window from unrelated work was open |
| Other local profiles | Windows Settings > Accounts > Other users showed no additional account; File Explorer showed only `aiwoperator` and `Public` under `C:\Users` |
| Nora browser/WAM sign-in | Not started |
| Signed app installation or visual acceptance | Not started on this VM |

The OS meets the selected Windows 11 x64 platform scope. The active
`aiwoperator` desktop is not the clean, separate standard-user profile required
for Nora's acceptance. Preserve its unrelated work. A disposable standard-user
Windows profile must be created and logged into normally before the credential
cache and first-run checks can be meaningful. Nora must complete her own normal
browser/WAM prompts; do not collect a password, code, authorization URL, or token.

The latest private signed application source remains `d21634f`, while the
merged current-account/target-status change is on main at `a53faf9`. A run of
the older signed build would remain historical evidence for `d21634f`. Before
claiming acceptance of the new wizard behavior, produce and independently
verify a private signed installer from the selected current source, then record
its source, hashes, installed payload, Windows build, and user context. The
release decision stays **no-go**.

Next operator sequence: establish the disposable standard-user desktop; verify
it has no prior app or Azure tool credential state; create the current-source
private signing candidate under the repository's explicit signing gate; verify
the download and installed payload; then run the
[clean-machine authentication checklist](windows-acceptance-status.md#clean-machine-authentication-checklist).
