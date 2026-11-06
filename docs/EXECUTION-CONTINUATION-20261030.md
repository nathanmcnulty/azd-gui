# PR-sized milestones after Graph session qualification

Checkpoint: 30 October 2026. This supersedes the work ordering in the
[September continuation](EXECUTION-CONTINUATION-20260927.md), while preserving
its historical acceptance records. It grants no cloud, consent or publication
authority by itself.

## Ready to work

Windows native compilation, the frontend toolchain, PowerShell and the managed
Graph adapter build are available. Further installer or account discovery is not
a prerequisite for local implementation. Follow the workspace instructions when
preparing a new build shell; do not change machine-wide defaults.

The local Graph foundation is based on PR #172 at `61b8b317`:

- `24d6dbb` owns Graph authentication and registered reads in one process,
  validates exact identity and actual grants, and disposes stale/cancelled sessions.
- `c896fac` retains the expected username hint without forcing an account picker.
  Its adapter regressions and desktop build passed; its prompt change was not
  live-retested. Keep operational qualification receipts private and bound to
  their exact tested executable.

This is a **draft PR candidate**, not a release candidate. The current foundation
is already a substantial reviewable change; it need not absorb every template
integration before review.

## Close the foundation as one focused PR

Proposed title: **Own Graph permission evidence in its PowerShell process**.
Stack it on `codex/conditional-requirement-snapshots-42` / PR #172. Preserve
the #166 → #172 dependency order. The #164 → #167 report-viewer stack and
dependency drafts #165/#171 remain separate; do not merge them incidentally.

The PR must explain the execution limit prominently: the worker verifies only
its registered reads. Generic Graph template initialization, lifecycle commands
and terminal handoffs remain blocked. A separate hook cannot borrow its proof.
This is a deliberate enforcement boundary, not evidence that template execution
has been implemented. See [the Graph boundary](graph-session-boundary.md).

Reuse the source-qualified frontend/native checks and independent review for
unchanged code. The later prompt adjustment has its own focused adapter checks
and desktop build. Do not claim that those checks repeat a full signed Windows
acceptance or that the current branch has passed every release audit.

## Recommended next milestone: a PIM read-only plan

**User outcome:** from the Windows GUI, select a pinned PIM source, choose plan
mode with Azure extensions disabled, approve its exact read requirements, run
the template-owned planner, and inspect its resulting plan. No directory or
Azure resource changes are part of this milestone. Generating this plan must
not require an Azure deployment or an azd provisioning lifecycle.

The inspected consumer baseline is azd-pim `691ecf4d`. Its current `azure.yaml`
starts separate interactive pre/post-provision hooks, its manifest requests a
static set including write scopes, and its planner reads through Graph SDK
context. Running that lifecycle under a read-only label is not the proposed path.

Deliver one coherent GUI PR, with a separate pinned consumer PR if adoption
requires changing azd-pim. Do not duplicate the PIM planning engine in the GUI.

Acceptance:

1. Define and implement one supported, registered plan-operation boundary. The
   process issuing protected Graph requests owns and validates its credential;
   another process's sign-in status cannot authorize it. Resolve this boundary
   with the smallest offline adapter proof before broad UI wiring.
2. Reuse the consumer's plan generation/report logic. Review its actual queries
   and map only their required read permissions into the effective contract.
   Plan mode requests no Graph write permissions. Optional emergency-group,
   notification, session-revocation and Azure features remain disabled initially.
3. Bind source, configuration, identity and requirements to the operation and
   displayed result. Editing them, cancelling or replacing the session discards
   stale evidence and late results. Show plan completion separately from apply
   or deployment success; retain the plan in a bounded local artifact.
4. Exercise the actual PowerShell/managed boundary with offline dependencies and
   template-compatible fixtures. Cover wrong identity, missing/changed grants,
   pagination/resource boundaries, cancellation, partial output and stale results.
   Forged enforced mode or a write dispatch must never reach a protected write.
5. Run focused regressions while implementing, then the appropriate integrated
   checks once on the final source. Obtain proportionate independent review and
   retain exact source/build evidence. Hand off a clean local commit and a
   concrete draft PR description with companion/dependency ordering.

No general shell/PTY, credential forwarding, profile injection, hook rewriting,
arbitrary renderer-selected Graph URL, blanket execution-guard removal or second
deployment engine is an acceptable shortcut. If the chosen consumer cannot adopt
the supported boundary, present concrete alternatives before expanding scope.

## Decisions that do and do not block the milestone

| Work | Can proceed locally? | Separate decision or approval |
| --- | --- | --- |
| Adapter design, source, fixtures, UI and validation | Yes | Routine implementation choices stay with the implementer |
| Reviewable commits and draft PR material | Yes | Publishing/pushing remains a separate scoped action |
| A bounded live plan read | After the source is reviewable | Exact account/client, read APIs/counts, scopes and cleanup packet |
| A dedicated restricted public client | Proposal only | App registration and consent changes need explicit approval |
| PIM enforcement, Azure extensions, role changes or cleanup | Outside this milestone | New operation scope and exact mutation approval |
| Signed beta/release acceptance | Separate workstream | Exact candidate, standard-user device and required human gates |

A dedicated client can isolate consent from the existing shared client, but its
absence does not block offline implementation. Recover established selectors from
private workspace records rather than asking for the same identifiers again.
Never add permissions merely to obtain a passing test.

## Milestone working agreement

The PIM read-only milestone is implemented on `codex/pim-read-plan`; see
[the workflow and validation contract](PIM-READ-ONLY-PLAN.md). It pins the companion
template change `97054a7c8bb67c4a4de52caccb68442c1e1ce769`. Offline qualification
does not authorize new sign-in, consent, live tenant reads or publication. The
private handoff records final validation, review and dependency ordering.

Take an agreed user outcome through design, implementation, focused validation,
review, local checkpoint and a PR-ready handoff. Reuse completed setup and
unaffected evidence. Ask only for a material scope/architecture choice, missing
information that cannot be recovered, an uncertain external operation, or an
action outside the agreed authority. Prepare the exact reviewable action before
asking for its approval. A milestone is complete when its acceptance criteria
are met, not when every adjacent release or template feature is finished.
