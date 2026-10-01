# Execution status

Next work is sequenced in the [27 September continuation roadmap](EXECUTION-CONTINUATION-20260927.md).

Updated 1 October 2026.

The second account-choice packet in
[PR 136](https://github.com/nathanmcnulty/azd-gui/pull/136) adds read-only, bounded Azure CLI and Az
PowerShell profile hints in Connections and the wizard. It enumerates provider
metadata only, offers no account-switch action, and cannot establish readiness
or permission. Account/context changes discard old hints and pending results.
Local validation passed 143 frontend and 163 native tests (three native
integration tests ignored), checks/build, formatting, and strict Clippy.
Independent review found no blocking issue. Read-only host probes returned two
Azure CLI entries and six Az PowerShell contexts; both provider defaults stayed
unchanged. The npm high-severity audit gate and Cargo audit passed; npm reported
one existing moderate `fast-uri` advisory for separate dependency maintenance.
No hosted workflow, signing run, Azure mutation, or Nora sign-in was started.
The new application code still requires a private signed candidate and affected
Windows acceptance. P7 remains release no-go.

A separate 1 October dependency patch in
[PR 137](https://github.com/nathanmcnulty/azd-gui/pull/137) updates the development-only transitive
`fast-uri` lock from 3.1.7 to 3.1.8 for
[GHSA-hrr3-gc8f-f4qj](https://github.com/advisories/GHSA-hrr3-gc8f-f4qj).
No dependency ranges or application behavior changed. A fresh lockfile install,
full local verify (143 frontend and 163 native tests; three ignored), and
dependency audits passed with zero npm vulnerabilities and no Rust advisories.

The [28 September RDP VM readiness check](windows-vm-readiness-20260928.md)
found Windows 11 Pro 25H2 x64 build `26200.9457` and only the local
`aiwoperator` profile. The VM has no separate standard-user desktop for Nora's
clean-profile acceptance. No account, app, prerequisite, or authentication
change was made during this check. PR 132's merged source still needs a new
private signed candidate before its changed wizard behavior can receive
qualifying Windows acceptance. P7 remains release no-go.

The first account-choice packet was implemented from main `f2233b5` in
[PR 132](https://github.com/nathanmcnulty/azd-gui/pull/132). The active desktop is
Windows 11 build 28000 under `azuread\nathanmcnulty`; no Nora standard-user
session or clean profile was available, so the human Windows gate remains open.
The packet shows each required provider's effective current account, fresh
target-context result, and next action in Connections and review. A cached
target belonging to another active account cannot appear ready. Target or
sign-in changes invalidate displayed readiness and Graph-grant proof before
rechecking. This is an application-code change after signed source `d21634f`;
it does not transfer any signed-candidate acceptance result to the new source.
Local `npm run verify` passed: zero Svelte/TypeScript diagnostics, 139 frontend
tests, production build, Rust formatting and strict Clippy, 158 native tests
with three integration tests ignored, and doc tests. The dependency audit
found no npm or Rust advisories. No hosted workflow,
signing run, Azure operation, or Nora sign-in was started for this packet.

The architecture/security checkpoint's three P2 repairs are merged on main at
`3988712`: Graph grant identity binding (PR 123), native Azure-target approval
binding (PR 124), and preparation/local-data-clear coordination (PR 125).
Integrated validation passed 138 frontend and 157 native tests (3 native
integration tests ignored), native formatting and Clippy, and dependency audits
with no findings. A final independent review found an additional native
environment-edit race, fixed in PR 128 at `da0a55f`; a follow-up Sol review
found no remaining app-controlled P1/P2 defect and the full local verify passed.
The first private signing [workflow 36296977843](https://github.com/nathanmcnulty/azd-gui/actions/runs/36296977843)
was cancelled before Azure signing when the race was found. Corrected source
`d21634f` passed [workflow 36298076501](https://github.com/nathanmcnulty/azd-gui/actions/runs/36298076501)
and independent host download verification. Fresh Windows 11 Sandbox build 28000
then installed and started the exact signed payload under a new non-admin user,
with valid installed hash/signature; this is process-level, not visual GUI proof.
The remaining release gate is the Windows 11
clean-profile journey, including Nora's browser/WAM sign-in and target rejection.
The proposed multi-account wizard scope is in
[Account choice in the deployment wizard](account-selection-design.md); it is
roadmap work, not completed behavior.

| Packet | Status | Baseline | Result |
| --- | --- | --- | --- |
| P1 — Make Windows acceptance finite and reproducible | **Complete** | `8d238405643c62bbdb667e157ec3ad0799a08963` | Current gate matrix, clean-machine browser/WAM checklist, and historical-installer clean-install proposal added. |
| P2 — Give a new Windows admin one clear next action | **Complete** | `5e434801a19db0451b63054e2bad38fd1d2d495c` | Blocked steps name the failing tool or account and its next action; unavailable accounts are distinct from verified target mismatches; compact wizard steps remain scrollable. |
| P3 — Explain external and existing-project trust accurately | **Complete** | `4284f0fff388a8da754b57a39b2fd9fd424fc701` | Source states, revision and digest limits, mutable working trees, re-review routes, and contract-bound preparation/lifecycle approvals are explicit and fail closed. |
| P4 — Qualify one useful solution template | **Complete** | `d15da58a4151009d06eca908c5680033c7c07084` | The pinned Maester Automation Account revision is mapped to every guided choice, permission boundary, cost driver, redeployment effect, evidence source, and cleanup limit; defaults, conditional gates, and review invalidation are covered. |
| P5 — Make management evidence useful | **Complete** | `66b006b793867ddd0db4014dde71290a3971173a` | azd-reference merge `08e6088` produces bounded 1.1 evidence; the GUI correlates it to private project, target, source, and operation state while preserving honest legacy behavior. |
| P6 — Prove recoverable solution management | **Complete** | `922772f63b1f00f3e7f81aeb646e7a06631b0d43` | Early and unrelated events retain exact operation identity; interrupted work reopens as attention; history names its environment relationship and cross-environment retry stays blocked. |
| P7 — Controlled pilot and release decision | **Live pilot complete; release no-go** | `d21634f063d259abf8495a1dc106ec3ca464ee3a` | Maester commit `45f839bc` completed the authorized Minimal deploy, validation, and teardown. The final reviewed private GUI candidate is signed and has passed independent download checks plus fresh Windows 11 non-admin installation/process startup. Earlier signed candidate passed prerequisite UI checks; exact-current-source visual, authentication, target rejection, and lifecycle gates remain open. |

P4 changed descriptive template guidance, its exact-revision qualification,
and regression coverage only. It started no authentication, cloud deployment,
signing workflow, release mutation, or native operation.

P4 validation:

- `npm run verify`: 128 frontend tests and 138 native tests passed; 3 native
  integration tests remained explicitly ignored;
- `npm run audit:dependencies`: npm reported no vulnerabilities and Cargo audit
  reported no advisories;
- focused coverage proves guided/default and Minimal choices, optional gates
  disappearing and reappearing, and review invalidation after applying a
  recommendation;
- the exact source review is [Maester Automation Account qualification](maester-automation-qualification.md).

P3 changed local renderer and native trust enforcement only. It started no
authentication, cloud deployment, signing workflow, release mutation, or native
operation.

P3 validation:

- `npm run verify`: 126 frontend tests and 138 native tests passed; 3 native
  integration tests remained explicitly ignored;
- `npm run audit:dependencies`: npm reported no vulnerabilities and Cargo audit
  reported no advisories;
- focused tests cover external review normalization, same-revision contract
  replacement, stale lifecycle approval, wizard digest propagation, legacy
  recovery, and the interactive-terminal boundary;
- final Sol high review reported no remaining P1 or P2 findings.

Pending work is finite and recorded in
[Windows acceptance status](windows-acceptance-status.md): Nora's human
browser/broker completion, a controlled target mismatch, affected clean-profile
UI checks, and an exact final tagged candidate. The maintainer has accepted
Windows 11 x64 beta scope and clean installation after the invalid historical
installer. The [26 September native follow-up](windows-acceptance-20260926.md)
records the prerequisite-display failure and verifies its fix in a corrected
signed build under fresh standard-user accounts.

The [26 September architecture and security checkpoint](architecture-security-checkpoint-20260926.md)
found no established P0 or P1 issue and retained the current architecture.
Those P2 packets are now merged as recorded above. Release-source signing and
affected Windows acceptance are the next gates.

P5 changed the azd-reference producer contract in PR 44 and the local GUI reader.
Bound evidence is current only when project, selected environment, target, saved
source contract, and exact persisted operation all agree. Legacy 1.0 evidence
remains readable as unbound; mismatched, stale, incomplete, malformed, and future
versions cannot produce a positive workspace status. The renderer receives only
bounded states, counts, and allowlisted next-action enums.

P5 validation:

- azd-reference: 254 Pester tests, PSScriptAnalyzer, component-version checks,
  and hosted Windows/Ubuntu validation passed at merge `08e6088`;
- azd-gui: `npm run verify` passed 133 frontend and 151 native tests, with 3
  native integration tests explicitly ignored;
- `npm run audit:dependencies` reported no npm or Rust advisories;
- final Sol high review reported no remaining P1 or P2 findings.

The published azd-reference `v0.2.0` receipt and `v1.1.0` validation tags remain
unsigned historical releases and were not retargeted. Their replacements,
`deployment-receipt@0.2.1` and `deployment-validation@1.1.1`, are signed GitHub-
verified tags at reference merge `0c96cc89c554ffc3b3ca82ceda12da6591e816c1`.
Both release-attestation workflows passed, and Maester commit
`45f839bc2c335cd1c7fa4c40ffe9c6978f715362` now vendors their exact locked bytes.

P6 reused existing deterministic coverage for partial preparation, preview and
deployment failure, cancellation, unsaved edits, environment switching, retry,
and teardown. It fixed the remaining identity and restart gaps: unknown events no
longer fall back to the selected project, early terminal events are replayed only
after their project and environment are registered, cancellation is reserved
before an operation ID becomes visible, and app closure leaves a recoverable local
attention record. Interrupted context stays private and usable for history scope,
but cannot make producer evidence current. Retained operations are labeled as the
current, another, or an unknown environment; only current-environment history can
be retried.

P6 validation:

- `npm run verify`: 135 frontend tests and 153 native tests passed; 3 native
  integration tests remained explicitly ignored;
- focused operation, catalog recovery, controller, persistence, immediate-cancel,
  and reopen recovery tests passed;
- `npm run audit:dependencies` reported no npm or Rust advisories;
- final Sol high review reported no remaining P1 or P2 findings.

P7 selected GUI source `a769ae6` and completed the authorized private signing
gate in [workflow 35670536219](https://github.com/nathanmcnulty/azd-gui/actions/runs/35670536219).
The workflow passed dependency audits, 135 frontend tests, Rust formatting and
Clippy, 153 native tests, release build, Azure OIDC, Trusted Signing, checksum
creation, installed-payload verification, silent install/uninstall, and private
artifact upload. A separate download check on Windows build 28000 verified both
artifact hashes, version 0.1.1, the expected publisher and trusted timestamps,
then repeated the custom-directory install, payload verification, and uninstall.
A clean, network-disabled Windows Sandbox build 28000 run against predecessor
`3971cb1` installed and verified its payload, then stopped at the expected native
missing-WebView2 dialog without rendering simulated application state. The delta
to `a769ae6` changes catalog and documentation only, so the evidence still covers
the unchanged bootstrap behavior; it does not replace exact-candidate or tagged-
artifact acceptance. The Sandbox account was elevated, so this closes only the
fail-closed bootstrap gate.

The Maester Minimal pilot subsequently completed in `westus2`; see the
[live pilot record](maester-minimal-pilot-20260921.md). It proved the authorized
resource, identity, report, evidence, and cleanup lifecycle without enabling any
optional component. Minimal permissions also exposed expected missing-permission
and `Forbidden` test output from the broader Maester suite, so the four passing
template checks are lifecycle evidence rather than a claim that every Maester
test passed.

The release decision remains **no-go** until the finite gates in
[P7 Windows release decision](p7-release-decision-20260920.md) are owned and
completed. The exact proposed cloud boundary is now captured in the
[Maester Minimal pilot authorization packet](maester-minimal-pilot-authorization.md),
including the eight baseline Graph application roles, a US$10 ceiling, stop
conditions, evidence, and independent cleanup checks. Its source commit is now
fixed at `45f839bc2c335cd1c7fa4c40ffe9c6978f715362` with the attested component
patches pinned and reviewed. The authorized live deployment and cleanup are now
complete. No application tag, draft release, or publication was performed.

The live pilot exposed excessive Automation progress replay and ambiguous
completed-job error output. Maester PRs 22 and 23 fixed the bounded presentation
and evidence language through `1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff`;
repository validation, dependency review, and CodeQL passed. The bundled catalog
now targets that commit. The live resource evidence remains bound to predecessor
`45f839bc`; private signed candidate `a769ae6` contains the new catalog pin, but
its Maester lifecycle has not been redeployed.
