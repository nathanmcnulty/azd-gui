# P7 Windows release decision

Decision date: 20 September 2026 (America/Los_Angeles).

## Decision

**No-go for tag, draft release, or publication.** The application is locally
code-complete through P6 at GUI main
`b008e2e27060098db14dd1388ba6b9995469ad73`, but the adopted Maester solution
has not completed an authorized live lifecycle and this exact application source
has not been signed or exercised on the clean acceptance image.

This is a finite external-gate decision, not a code-quality failure. No cloud
resources, directory objects, signing operations, tags, or releases were created
while reaching it.

## Exact local candidate

| Item | Result |
| --- | --- |
| GUI source | `b008e2e27060098db14dd1388ba6b9995469ad73` |
| Reference evidence producer | signed patch releases `deployment-receipt@0.2.1` and `deployment-validation@1.1.1` at `0c96cc89c554ffc3b3ca82ceda12da6591e816c1`; both attestations passed and Maester `45f839bc2c335cd1c7fa4c40ffe9c6978f715362` pins their exact bytes |
| Version alignment | `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` all report `0.1.1` |
| Dependency install | clean `npm ci`; 141 packages audited, no npm vulnerabilities |
| Integrated checks | 135 frontend tests and 153 native tests passed; 3 native integration tests remained explicitly ignored |
| Dependency audit | no npm or Rust advisories |
| Windows release compile | `npm run tauri -- build --no-bundle --ci` passed |
| Local executable | product/file version `0.1.1`, SHA-256 `b6f44c3946f8521dc186f8ba486836ff06d0a7e6debf7590b393307255148d9c` |
| Signature boundary | `NotSigned`, as expected for the local compile; this file is not a release artifact |
| Independent review | final Sol high P6 review found no remaining P1 or P2 findings |

The local executable hash is reproducibility evidence for this workstation only.
It must not be compared with or substituted for a workflow-signed portable or
installer payload.

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
3. **Signing owner:** explicitly confirm the metered Windows runner and Azure
   signing operation for this exact source. A manual signing run may produce private
   workflow artifacts; a tag and draft release remain separate decisions.
4. **Windows acceptance owner:** verify the downloaded signer identity, checksums,
   installed payload, affected admin surfaces, persistence schema 4, interruption
   recovery, output/history redaction, uninstall, and the supported Windows build.
   Decide whether Windows 11 evidence is sufficient for beta or run Windows 10.
5. **Maintainer:** accept the documented clean-install migration for the invalid
   historical public installer or choose another reviewed remediation before
   publication.

## Next authorized sequence

The component patch releases are now pinned in the exact solution revision. After
Maester pilot authorization, run its complete
preview, deploy, reopen, evidence, safe retry/cancel, down, and independent ARM plus
non-ARM cleanup path. Settle the resulting GUI source, then run one explicitly
confirmed signing workflow. Verify and install its private artifacts before any tag
or public draft decision. Reconcile every changed row in the acceptance matrix; do
not carry forward a pass merely because version `0.1.1` stayed the same.
