# P7 Windows release decision

Decision date: 20 September 2026 (America/Los_Angeles).

## Decision

**No-go for tag, draft release, or publication.** The exact application source
`a769ae68b7db8c4b118374026412bab54dc73796` has passed private Windows signing,
download, installation, payload, and uninstall verification. The adopted Maester
solution subsequently completed its authorized Minimal lifecycle at commit
`45f839bc2c335cd1c7fa4c40ffe9c6978f715362`; see the
[live pilot record](maester-minimal-pilot-20260921.md). The candidate has not
completed its affected clean-profile authentication and UI acceptance.

This is a finite external-gate decision, not a code-quality failure. The signing
operation produced a private 14-day workflow artifact only; it created no app
tag, draft release, or public release.

## Exact signed candidate

| Item | Result |
| --- | --- |
| GUI source | `a769ae68b7db8c4b118374026412bab54dc73796` |
| Reference evidence producer | signed patch releases `deployment-receipt@0.2.1` and `deployment-validation@1.1.1` at `0c96cc89c554ffc3b3ca82ceda12da6591e816c1`; both attestations passed, bundled Maester `1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff` pins their exact bytes, and live lifecycle evidence remains bound to predecessor `45f839bc2c335cd1c7fa4c40ffe9c6978f715362` |
| Version alignment | `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` all report `0.1.1` |
| Dependency install | clean `npm ci`; 141 packages audited, no npm vulnerabilities |
| Integrated checks | 135 frontend tests and 153 native tests passed; 3 native integration tests remained explicitly ignored |
| Dependency audit | no npm or Rust advisories |
| Private signing workflow | [run 35670536219](https://github.com/nathanmcnulty/azd-gui/actions/runs/35670536219) passed all build, test, Azure OIDC, signing, artifact, installed-payload, and uninstall gates |
| Downloaded portable | product/file version `0.1.1`; SHA-256 `06dc34656e9be603e00e8f8744053ec0b9ae1a265fd9fe1a66be7e30c11cc861` |
| Downloaded NSIS installer | SHA-256 `5d227600c82827af65a0640c13b810bca076c7c7b35424a8d007bd62173a80dd` |
| Installed executable | SHA-256 `70923c1dcd524d0e97fac80664aefad5674abf6586ebe61b19f870accfdf1ed8`; runner and host verification passed |
| Signature boundary | Both downloaded artifacts and the installed payload have valid trusted timestamps and signer `CN=Nathan McNulty, O=Nathan McNulty, L=Soldotna, S=Alaska, C=US` |
| Missing-WebView2 boundary | A clean, network-disabled Windows Sandbox build 28000 installed predecessor `3971cb1` and stopped at the native missing-runtime dialog without simulated application state. The delta to `a769ae6` changes catalog and documentation only; an eventual tagged artifact must repeat this check. Its elevated account does not satisfy standard-user acceptance |
| Independent review | final Sol high P6 review found no remaining P1 or P2 findings |

The artifact remains private and expires with the workflow retention period. A
future tagged build is a new artifact boundary and must repeat verification.

## Gates that require an owner

1. **Maester pilot:** **complete.** The owner authorized the exact boundary in the
   [Maester Minimal pilot authorization packet](maester-minimal-pilot-authorization.md),
   and the [live pilot](maester-minimal-pilot-20260921.md) completed deployment,
   validation, evidence capture, teardown, and independent ARM plus Graph cleanup.
2. **Authentication operator:** supply a disposable identity, expected target, and
   controlled mismatched target, then complete normal browser/WAM prompts using the
   checklist in [Windows acceptance status](windows-acceptance-status.md). Device
   code remains prohibited.
3. **Windows acceptance owner:** exercise the affected admin surfaces,
   persistence schema 4, interruption recovery, and output/history redaction on
   a clean profile using this exact candidate. Decide whether Windows 11 evidence
   is sufficient for beta or run Windows 10. The artifact and host installation
   checks are complete.
4. **Maintainer:** accept the documented clean-install migration for the invalid
   historical public installer or choose another reviewed remediation before
   publication.

## Next authorized sequence

The component patch releases are pinned in the exact solution revision, and the
authorized Maester lifecycle is complete. Complete clean-profile authentication
and the affected UI checks against the private candidate. If that path changes
application code, select the resulting source and repeat signing. Reconcile every
changed row in the acceptance matrix before any tag or public draft decision.
