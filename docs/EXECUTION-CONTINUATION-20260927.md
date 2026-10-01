# Windows acceptance and account-choice continuation

Roadmap checkpoint: 27 September 2026; candidate handoff updated 1 October 2026.
This is the active execution order after P1–P7
of the [original plan](EXECUTION-PLAN.md). Keep the architecture, validation,
model policy, and milestone handoff rules there. The [status](EXECUTION-STATUS.md)
and [Windows gate matrix](windows-acceptance-status.md) are the evidence records;
update them after each qualifying attempt. This plan does not turn a historical
pass into proof for a newer source or machine.

## Where we are

- P1–P6 are merged. The Maester Minimal pilot deployed, validated, and cleaned
  up under its separately documented boundary. P7 remains **release no-go**.
- Current private signed 0.1.1 application source is
  `b018ed257917a7c5118c8bf1665c9820babda244`, from
  [signing run 36914798448](https://github.com/nathanmcnulty/azd-gui/actions/runs/36914798448).
  Both account-choice packets are merged. The signed source passed hosted
  audits/tests/build, signatures and installed-payload checks, and independent
  host download verification. The [1 October VM preparation](windows-acceptance-preparation-20261001.md)
  records the desktop kit and prerequisites. The older `d21634f` standard-user
  Sandbox install/startup remains historical; no new-profile installation,
  visual wizard acceptance or Nora sign-in is established for `b018ed2`.
- Private downloaded artifacts and receipts are under
  `C:\Users\NathanMcNulty\.codex\artifacts\azd-gui-b018ed2-signed` and the
  acceptance store named in the [26 September record](windows-acceptance-20260926.md).
  Keep raw authentication state and private receipts outside Git.
- The gate matrix has 17 Passed, 5 Not run, 3 Blocked, and 1 Failed entries.
  The failed historical public `v0.1.0-alpha` installed payload stays a failed
  in-place-upgrade gate; the accepted path is clean installation. There is no
  0.1.1 tag, draft, or public release.

## A. Finish the Windows 11 human gate

Use a full, normal Windows 11 standard-user desktop for the selected test user.
Avoid relying on a secondary account started inside Sandbox: its previous azd
browser handoff failed even though Edge could launch. Reuse the exact signed
candidate above and record its installer and installed-executable hashes,
Windows build, user context, and results. Windows 10 is out of beta scope.

1. The maintainer completes the normal browser/WAM prompts for
   `nora@sharemylabs.com` in the selected desktop. Expected tenant is
   `847b5907-ca15-40f4-b171-eb18619dbfab` and subscription is
   `a80941e8-c2b9-4bc9-83ad-117cc40d0bea`. Read-check her current access
   before relying on temporary Reader. Do not request or retain a password,
   one-time code, authorization URL, or raw token. Never use device-code auth.
2. Visually observe current-source Home, Connections, Tools, gallery/source
   review, wizard, exact target and approval, missing-prerequisite recovery,
   and draft reopen. Compare displayed provider identity and target with fresh
   probes; a visible cached account is not authorization.
3. Test an unavailable subscription UUID as a read-only negative target.
   Continue and deployment must remain blocked. This proves unavailable-target
   rejection, not cross-tenant isolation for every possible tenant.
4. Exercise pinned starter preparation, one cancellation and partial-project
   recovery, and the interactive terminal handoff using only documented safe
   paths. Inspect output/history, persisted draft, and reopened app for a
   harmless sensitive marker. Record clear-local-data and credential-cache
   preservation only in an explicitly disposable signed-in profile.
5. If the chosen solution's full GUI deploy/verify/down lifecycle is still
   needed, use the separately authorized Maester Minimal boundary and budget;
   verify exact Azure and Graph cleanup independently. Do not expand resource
   scope or treat the earlier CLI-host pilot as GUI proof.

Stop and record the exact gate and cause when an interactive step cannot
proceed. Do not substitute a host cache, an automated test, or an older signed
candidate for clean-profile evidence. Update the gate matrix and release
decision before claiming beta readiness.

## B. Independent improvement while the human gate waits

The first implementation packet from
[account-choice design](account-selection-design.md) can proceed as one focused
PR: show the **effective account and fresh target-check result per required
provider** on the wizard's existing Connections/review path. For an unavailable,
wrong-target, expired, or unverified provider, show the specific next action;
on account or target change, discard previous readiness and Graph-grant proof.
Use existing native probes and bounded typed results. Include focused tests for
no account, wrong active account, expiry/probe failure, and target change.
Preserve fail-closed approval and no-secret-persistence behavior.

Do not introduce a general token manager, parse CLI token caches, silently
change CLI defaults, or infer access from a profile label. Optional locally
visible profile hints are a **second** milestone only after packet B is
reviewed; provider-specific explicit reconnect/selection is a **third**.
Follow the design's provider limitations and native browser/WAM flow. Keep
each packet separate so security review can assess actual changed behavior.

## C. Candidate and release checkpoint

After app code settles, run the appropriate local verify/audits and hosted CI,
then make **one** new private signing candidate if application, packaging, or
workflow changes require it. Recheck affected Windows gates against that exact
source and installed payload. Documentation-only changes do not require
resigning. An independent auth/security review is warranted for any new
account switching or approval-boundary change; use Astra only for an unresolved
architecture or security decision that Sol/Terra cannot settle.

Record the final gate matrix, any accepted exception, installer provenance,
and cleanup evidence. Decide on a tagged draft only after exact-candidate
acceptance; verify downloaded draft assets and installed payload again before
publication. Never promote the historical invalid installer to an upgrade pass.

## Working agreement for the next session

Use Sol at medium effort to own the milestone; delegate bounded code/tests to
Luna or multi-file reasoning to Terra only when parallel work materially helps.
Use the `nathanmcnulty` GitHub account and a `codex/` branch. Validate the
focused change, open a PR, merge at a good milestone, and verify `main`. Preserve
unrelated work and stop at the milestone with a compact evidence update. Avoid
repeating cloud deployment or Azure signing merely to reconfirm unchanged code.

If the full Windows desktop and Nora are ready, begin **A** and complete the
human acceptance record. B's first and second packets are now merged; the third
explicit account-transition packet should follow the real multi-account Windows
trial in the account-choice design. The next human action is to create and log
into the new standard-user VM desktop, install the verified desktop candidate,
and complete Nora's normal interactive sign-in. The kit contains the human
handoff and helpers; never ask her to share credentials or codes.

Suggested new-session prompt:

> Read `docs/EXECUTION-CONTINUATION-20260927.md`, the current status and Windows
> gate matrix in `E:\azd-gui`. Continue the first available packet: Windows 11
> Nora clean-profile acceptance if the desktop is ready, otherwise the focused
> current-account/target-status wizard improvement. Preserve the signed-source
> evidence boundary, use `nathanmcnulty`, PR and merge a validated milestone,
> and update the evidence docs. Do not use device-code authentication.
