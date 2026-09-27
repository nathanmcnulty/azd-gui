# Windows beta architecture and security checkpoint

Review date: 26 September 2026. Reviewed clean `main` commit
`6fbb61829757372a132529b28919e6d31587c40f`. The private signing workflow
passed on application source `cc67ad9378c053af6b97ab7df0bb5881c94ad5b1`;
the intervening commits changed documentation only. This was a read-only code
review, not a new live deployment or signed-candidate acceptance test.

## Verdict

Keep the existing Svelte/Tauri/Rust architecture. Typed commands, pinned source
review, one-use native approvals, explicit environments, bounded evidence
readers, interactive-terminal routing, and signed payload verification are a
sound foundation for a Windows-first admin tool. No P0 or P1 issue was
established. Resolve the three P2 findings below before selecting a beta release
source, then rebuild and repeat affected signed Windows acceptance. These are
bounded repairs, not a reason to add another deployment engine or general shell.

## Ordered repair packets

1. **Bind Graph grant status to the same identity.**
   `src/lib/connections.ts` retains old `grantedScopes` when a refreshed Graph
   context is connected but reports no scopes. The native `.mg` auth-record
   fallback in `src-tauri/src/connections.rs` routinely reports no scopes.
   A read-only reproduction using the actual helper showed that changing from
   account A/tenant A to account B/tenant B can change B's target readiness
   from false to true by inheriting A's grants. This is a false readiness
   assertion, not a Graph authorization bypass. Retain grants only when both
   normalized account and tenant match and are known; otherwise show the grants
   as unverified and require a fresh check/sign-in. Cover changed account,
   changed tenant, missing identity, and same-identity fallback in focused tests.

2. **Bind the native execution approval to the Azure target.**
   `ExecutionApprovalRequest` in `src-tauri/src/models.rs` carries project,
   environment name, revision, and reviewed contract, but no tenant,
   subscription, or resource-group identity. The native confirmation in
   `src-tauri/src/lib.rs` does not display a resolved target. A saved `.azure`
   target can change between approval and `start_azd_operation` while the
   approval comparison still succeeds; azd then reads the changed target.
   This is a code-established approval gap, especially material for
   `down --force`; no live retargeting was attempted. Map the actual target
   source and precedence first, then resolve and show the canonical target
   natively at approval and compare it immediately before spawn. Apply the
   same binding to preparation's requested target. Fail closed when an expected
   target cannot be verified. Use a fake CLI to change the saved target between
   approval and start and assert that no lifecycle command starts. This does
   not claim that approved template hooks are contained after execution begins.

3. **Register preparation as active work.**
   `prepare_deployment` runs `deployment::prepare` outside `OperationManager`,
   while `clear_local_app_data` checks only that manager. During a slow
   checkout or `azd init`, clearing local data can remove the template cache
   and project registration, and preparation can later repopulate state.
   Guided preflight also does not mark the operation running until preparation
   completes, allowing a duplicate start. Reserve native preparation activity
   for its full lifetime, release it on every exit, and make clearing and
   duplicate preparation honor that reservation. Test a paused preparation,
   attempted clearing, duplicate start, and cleanup after failure/cancellation.

Each packet should produce a focused PR with its own regression proof. The
coordinator then runs the integrated checks once on the combined source, performs
a narrow security review of the final diff, signs that exact source, and retests
the affected Windows surfaces. Keep the [acceptance matrix](windows-acceptance-status.md)
honest: Nora's browser/WAM sign-in, controlled target rejection, remaining
clean-profile lifecycle gates, and eventual tagged-artifact verification are
still open. No tag or draft release is authorized by this review.

The reviewer also noted that generic output regexes cannot guarantee removal
of every arbitrary hook-emitted secret. No separate end-to-end leak was
established; keep this limitation in the output/history acceptance review
without broadening the three repairs above.
