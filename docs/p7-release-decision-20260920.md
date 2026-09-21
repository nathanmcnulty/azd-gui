# P7 Windows release decision

Decision date: 20 September 2026 (America/Los_Angeles).

## Decision

**No-go for tag, draft release, or publication.** The exact application source
`3971cb1f685eae93c48c0fa37972b226d29e286f` has passed private Windows signing,
download, installation, payload, and uninstall verification. The adopted Maester
solution has not completed an authorized live lifecycle, and the candidate has
not completed its affected clean-profile authentication and UI acceptance.

This is a finite external-gate decision, not a code-quality failure. The signing
operation produced a private 14-day workflow artifact only; it created no app
tag, draft release, or public release.

## Exact signed candidate

| Item | Result |
| --- | --- |
| GUI source | `3971cb1f685eae93c48c0fa37972b226d29e286f` |
| Reference evidence producer | signed patch releases `deployment-receipt@0.2.1` and `deployment-validation@1.1.1` at `0c96cc89c554ffc3b3ca82ceda12da6591e816c1`; both attestations passed and Maester `45f839bc2c335cd1c7fa4c40ffe9c6978f715362` pins their exact bytes |
| Version alignment | `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` all report `0.1.1` |
| Dependency install | clean `npm ci`; 141 packages audited, no npm vulnerabilities |
| Integrated checks | 135 frontend tests and 153 native tests passed; 3 native integration tests remained explicitly ignored |
| Dependency audit | no npm or Rust advisories |
| Private signing workflow | [run 35655808067](https://github.com/nathanmcnulty/azd-gui/actions/runs/35655808067) passed all build, test, Azure OIDC, signing, artifact, installed-payload, and uninstall gates |
| Downloaded portable | product/file version `0.1.1`; SHA-256 `8770f927399b1dc74492961831c2f0380607fc09e7a9e5154215a060aa2a870c` |
| Downloaded NSIS installer | SHA-256 `366202e23131ca6051ac770ae8f83ba24d5a90180d42b037c58efd05bac72039` |
| Installed executable | SHA-256 `f1df7ff009eaf0ef5face5ae546d36615b3e7c225905c463d2c6b12823e9e9d8`; runner and host verification passed |
| Signature boundary | Both downloaded artifacts and the installed payload have valid trusted timestamps and signer `CN=Nathan McNulty, O=Nathan McNulty, L=Soldotna, S=Alaska, C=US` |
| Missing-WebView2 boundary | A clean, network-disabled Windows Sandbox build 28000 installed the exact payload and stopped at the native missing-runtime dialog without simulated application state; its elevated account does not satisfy standard-user acceptance |
| Independent review | final Sol high P6 review found no remaining P1 or P2 findings |

The artifact remains private and expires with the workflow retention period. A
future tagged build is a new artifact boundary and must repeat verification.

## Gates that require an owner

1. **Maester pilot owner:** authorize the exact template revision, tenant,
   subscription, resource group/naming boundary, directory and service permissions,
   optional features, spending boundary, and cleanup owner. The prior empty-resource-
   group pilot does not authorize or prove this solution lifecycle. The proposed
   least-privilege boundary is recorded in the
   [Maester Minimal pilot authorization packet](maester-minimal-pilot-authorization.md).
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

The component patch releases are pinned in the exact solution revision. After
Maester pilot authorization, run its complete preview, deploy, reopen, evidence,
safe retry/cancel, down, and independent ARM plus non-ARM cleanup path. Complete
clean-profile authentication and the affected UI checks against the private
candidate. If either path changes application code, select the resulting source
and repeat signing. Reconcile every changed row in the acceptance matrix before
any tag or public draft decision.
