# Execution status

Updated 21 September 2026.

| Packet | Status | Baseline | Result |
| --- | --- | --- | --- |
| P1 — Make Windows acceptance finite and reproducible | **Complete** | `8d238405643c62bbdb667e157ec3ad0799a08963` | Current gate matrix, clean-machine browser/WAM checklist, and historical-installer clean-install proposal added. |
| P2 — Give a new Windows admin one clear next action | **Complete** | `5e434801a19db0451b63054e2bad38fd1d2d495c` | Blocked steps name the failing tool or account and its next action; unavailable accounts are distinct from verified target mismatches; compact wizard steps remain scrollable. |
| P3 — Explain external and existing-project trust accurately | **Complete** | `4284f0fff388a8da754b57a39b2fd9fd424fc701` | Source states, revision and digest limits, mutable working trees, re-review routes, and contract-bound preparation/lifecycle approvals are explicit and fail closed. |
| P4 — Qualify one useful solution template | **Complete** | `d15da58a4151009d06eca908c5680033c7c07084` | The pinned Maester Automation Account revision is mapped to every guided choice, permission boundary, cost driver, redeployment effect, evidence source, and cleanup limit; defaults, conditional gates, and review invalidation are covered. |
| P5 — Make management evidence useful | **Complete** | `66b006b793867ddd0db4014dde71290a3971173a` | azd-reference merge `08e6088` produces bounded 1.1 evidence; the GUI correlates it to private project, target, source, and operation state while preserving honest legacy behavior. |
| P6 — Prove recoverable solution management | **Complete** | `922772f63b1f00f3e7f81aeb646e7a06631b0d43` | Early and unrelated events retain exact operation identity; interrupted work reopens as attention; history names its environment relationship and cross-environment retry stays blocked. |
| P7 — Controlled pilot and release decision | **Live pilot complete; release no-go** | `3971cb1f685eae93c48c0fa37972b226d29e286f` | The exact GUI candidate is signed and privately verified, and Maester commit `45f839bc` completed an authorized Minimal deploy, validation, evidence, and independently verified teardown. Clean-account auth, affected clean-VM acceptance, and the historical-installer decision remain external gates. |

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

Pending input is finite and recorded in
[Windows acceptance status](windows-acceptance-status.md): a disposable identity,
expected and mismatched targets, human browser/broker completion, exact final
candidate, and the maintainer decision for the historical installer limitation.

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

P7 selected GUI source `3971cb1` and completed the authorized private signing
gate in [workflow 35655808067](https://github.com/nathanmcnulty/azd-gui/actions/runs/35655808067).
The workflow passed dependency audits, 135 frontend tests, Rust formatting and
Clippy, 153 native tests, release build, Azure OIDC, Trusted Signing, checksum
creation, installed-payload verification, silent install/uninstall, and private
artifact upload. A separate download check on Windows build 28000 verified both
artifact hashes, version 0.1.1, the expected publisher and trusted timestamps,
then repeated the custom-directory install, payload verification, and uninstall.
A separate clean, network-disabled Windows Sandbox build 28000 run installed and
verified the same payload, then stopped at the expected native missing-WebView2
dialog without rendering simulated application state. The Sandbox account was
elevated, so this closes only the fail-closed bootstrap gate.

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
completed-job error output. Maester PR 22 fixed that bounded presentation path at
`1692b2e5a69c414b1e204c2714281ba0085c3560`; repository validation, dependency
review, and CodeQL passed. The bundled catalog now targets that commit. The live
resource evidence remains bound to predecessor `45f839bc`, and adopting the new
catalog pin means a future signed GUI candidate must use the resulting GUI source.
