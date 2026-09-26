# Windows admin experience: execution plan

Planning baseline: 20 September 2026, GUI main `a4c1552`, application source
`179b89e`, signed candidate 0.1.1. This document authorizes no new execution by
itself. Work starts only when the user asks to follow it. No agents, application
changes, deployments, or signing runs are part of creating this plan.

## Outcome and architecture

An administrator can choose a gallery or external template, understand its source
and impact, answer relevant questions, review the exact Azure target, deploy through
supported CLIs, and return later to understand status, next actions, and cleanup.
Optimize this complete Windows journey before adding more gallery entries.

Keep the existing architecture. Svelte presents typed state; the bridge and small
controllers coordinate it; Rust owns process execution, source validation,
approvals, persistence, and bounded reads. Templates own infrastructure, feature
semantics, hooks, and verification. azd-reference supplies versioned contracts and
vendored components. Do not introduce a general shell/PTY, dynamic manifest
commands, a second deployment engine, or a new state-management framework.

Prefer fewer decisions and clear recovery paths. Use template-authored defaults,
profiles, conditional questions, and existing controls. Explain required roles,
optional components, and cost drivers beside the choice that causes them. Do not
invent prices, permissions, automatic recommendations, or unsupported CLI behavior.

## Model and quota policy

These are project assignments, not guaranteed pricing or quota ratios.

| Responsibility | Default model and effort |
| --- | --- |
| Own one milestone, integrate, update handoff | Sol (`gpt-5.6-sol`), medium |
| One bounded implementation, fixture, test, or documentation task | Luna (`gpt-5.6-luna`), medium |
| Coordinated changes across renderer, native layer, or template contracts | Terra (`gpt-5.6-terra`), medium |
| Review authentication, authorization, execution, persistence, or source trust | Sol, high |
| Resolve an architectural decision that remains ambiguous after a concrete proposal | Astra, occasional user-requested checkpoint |

Use Sol as the selected parent model rather than keeping Astra active to supervise
routine work. A small, well-defined task can be done directly by Luna without a
coordinator. Parallelism is optional: default to one implementer; add one independent
reviewer when risk warrants it. Do not assign two agents to edit `src/App.svelte`.

Specify both model and effort when spawning. Defaults/inheritance can otherwise
select a more expensive model; explicit spawn settings, configured defaults, and
custom-agent settings have documented precedence. See the
[official subagent guidance](https://learn.chatgpt.com/docs/agent-configuration/subagents).
With this task's collaboration API, model overrides require a bounded context fork
(prefer `fork_turns="none"` with the task packet); a full-history fork inherits the
parent. Check actual available models before dispatch; never silently substitute
Astra. No global agent configuration changes are required by this plan.

Cost controls:

- Read this plan, the current handoff, and the relevant files, not the full task
  history or whole repository. Re-inventory only when evidence has changed.
- One task packet should yield one reviewable result. Split contract decisions,
  implementation, and live acceptance into separate packets.
- Return concise findings, changed files, commit, checks, and remaining risks.
  Avoid duplicate full reviews or repeating a passing suite without a new reason.
- Let the implementer run focused checks; the milestone owner runs the required
  integrated gate once. Reuse build outputs only in their owning worktree/runtime.
- Do not keep a high-cost model active solely to poll a build. Save the run ID,
  exact source, artifact expectations, and next command, then yield for a later
  explicit continuation. No automatic monitor or new task is implied.
- After two unsuccessful attempts at the same defect, stop broad trial-and-error.
  Produce the smallest reproduction and escalate Luna → Terra → Sol. Escalate to
  Astra only with a specific unresolved design question and alternatives.

## Established baseline: preserve, do not redo routinely

The [acceptance record](windows-acceptance-20260920.md) is the evidence source.

| Established | Boundary |
| --- | --- |
| Named Azure deployment check starter pinned to reference `abc6e896620990e68360a63dd14efda66a3f75cd` | Empty resource group only; not a full application deployment |
| Native GUI prepare, preview cancel/retry, deploy, reopen, validate, down | Development build and cached host authentication; independent Azure cleanup confirmed |
| Signed 0.1.1 installation, corrected 0.1.0 upgrade, visual screens, draft/settings retention, data clearing, uninstall | Exact signed candidate; Windows 11 Enterprise build 28000; not all Windows versions |
| 118 frontend and 133 native tests, hosted validation and signing | Recorded counts, not a required future fixed count |
| Required/optional tools and checks, feature conditions, profiles, group picker, revision-bound drafts, report/receipt readers | Extend existing mechanisms; do not rebuild them |

Still open: clean-machine interactive authentication; the invalid installed payload
in the historical public installer; a real solution's optional-feature and
management lifecycle; stronger report correlation and existing-project trust UX.
The corrected-baseline upgrade must not be relabeled as a historical-installer pass.
Missing-WebView2 UI evidence is from an earlier verified baseline. Repeat relevant
gates when changing those behaviors or selecting a final release candidate.

## Start/resume procedure

1. Check branch, dirty state, worktrees, and current remote main. Preserve unrelated
   work. `E:/azd-gui` and its managed worktrees share a repository; older C: clones
   are independent. Do not reset or clean them to match this document.
2. Read applicable AGENTS instructions, this file, the latest acceptance record,
   and [Actions policy](actions-policy.md). Confirm which task below is unstarted.
3. For reference changes, inspect `E:/azd-reference` and its worktrees separately.
   Do not publish unrelated local commits. Inspect the exact pinned consumer source
   before changing its contract or claiming compatibility.
4. Choose the earliest unblocked packet. A blocked interactive/cloud gate does not
   block independent local design and implementation. Do not repeatedly ask for
   the same pending account or authorization.
5. Work one milestone, validate, review proportionally, commit/push and PR/merge
   under the user's standing authorization. Stop at the milestone and leave a
   compact handoff unless the new user instruction explicitly requests more.

## Ordered task packets

Packet progress is recorded in [EXECUTION-STATUS.md](EXECUTION-STATUS.md). “Ready”
means local work can begin after the user resumes execution, not that its live
actions are approved.

### P1 — Make Windows acceptance finite and reproducible

**Owner:** Luna medium; Sol reviews the release boundary. **Dependency:** none.
**Entry points:** `docs/windows-beta-acceptance.md`, the acceptance record,
`docs/actions-policy.md`, `scripts/verify-release-artifacts.ps1`.

Create one current matrix identifying each gate, exact artifact/source, evidence,
owner action, and status (passed/blocked/not run/failed). Reconcile historical
paragraphs with the newer evidence rather than deleting historical failures.
Prepare a short normal-browser/WAM sign-in checklist: no cached account, user
completes authentication, refresh status, select matching tenant/subscription,
and ensure mismatched context remains blocked. No cloud mutation is needed merely
to prove sign-in. Propose a documented clean-install migration path for the broken
historical package without launching its invalid payload or silently replacing
public assets.

**Done:** a later operator knows exactly which test and input remain; no repeated
full acceptance campaign is needed to discover this. Links and commands checked;
documentation-only work does not require recompiling the application. Live sign-in
remains blocked until an account and human interaction are available. Publication
and historical-release remediation decisions remain with the user.

### P2 — Give a new Windows admin one clear next action

**Owner:** Luna medium for copy/layout; Terra if state transitions must change.
**Dependency:** P1's matrix; no live login dependency.
**Entry points:** `src/App.svelte`, `src/app.css`, `src/lib/requirements.ts`,
`src/lib/wizard.ts`, `src/lib/controllers/connection-controller.ts`, corresponding
tests; `src-tauri/src/tools.rs` only if a demonstrated native defect requires it.

Walk the existing starter journey and record at most three concrete sources of
confusion. Fix them one at a time: missing tool guidance, missing account guidance,
or the next recoverable wizard action. Keep ordinary inputs primary and advanced
IDs/variables secondary. Distinguish tool missing, account disconnected, unknown
status, and wrong tenant. Preserve native approval and explicit source/target review;
do not remove safety checks to reduce clicks or add automatic installation.

**Done:** empty-profile and cached-profile walkthroughs show an actionable reason
for each blocked Continue; no false connected/ready/deployed state; keyboard and
small-window layout are usable. Add a behavioral regression when logic changes;
do not add tests that merely freeze copy. Limit this packet to the selected friction
points, not a broad redesign or App.svelte extraction.

### P3 — Explain external and existing-project trust accurately

**Owner:** Sol medium for a short decision, Terra for implementation, Sol high review.
**Dependency:** P2; local fixtures suffice.
**Entry points:** `src/components/CatalogSourceDialog.svelte`, `src/App.svelte`,
`src/App.catalog-recovery.test.ts`, `src/App.project-template.test.ts`,
`src-tauri/src/{catalog,local_source,deployment,execution_approval,persistence}.rs`,
`docs/{architecture,template-authoring}.md`.

First write a small state/wording table for bundled, external pinned, locally copied,
and reopened mutable projects. Show source URL/path, reviewed revision, digest
meaning, and whether the current working tree is mutable. Existing integrity
digests do not authenticate a publisher. Avoid an unqualified “trusted” or “vetted”
claim for mere digest consistency. Old registrations with insufficient provenance
need explicit re-review before new deployment actions; retain safe inspection and
necessary cleanup routes. Do not invent certificate infrastructure or silently
freeze existing project directories.

**Done:** fresh pinned preparation remains bound to reviewed content; changed
revision/manifest or stale response cannot reuse approval; legacy and changed
working-tree cases have explicit behavior. Regression tests cover those transitions
and preserve the interactive-terminal boundary. If detecting arbitrary working-tree
changes would require a new trust model, stop at the decision and ask Sol to narrow
it; do not build a generic integrity service.

### P4 — Qualify one useful solution template before widening the gallery

**Owner:** Terra medium for source mapping; Luna medium for a bounded manifest/test
change after that mapping; Sol high reviews permissions and destructive behavior.
**Dependency:** P2; P3 is required for new trust behavior, not for offline mapping.
**Entry points:** `catalog/manifests/maester-automation.json`,
`catalog/azd-gui.catalog.json`, `schemas/azd-gui.schema.json`,
`src/lib/{requirements,wizard}.ts`, `src/lib/schema.test.ts`,
`src/components/{ConfigurationFieldControl,WizardConfigurationReview}.svelte`.
Reference: `standards/gui-template-contract.md`, `tests/GuiTemplateContract.Tests.ps1`.

Use Maester Automation Account as the next candidate, not as already approved for
deployment. Inspect the bundled exact source revision first. Produce a mapping:
question → variable → default → feature condition → required tools/roles → impact
→ evidence/cleanup. Preserve its baseline roles and the external-terminal handoff.
The Minimal profile must not imply zero directory changes. Explain optional
Exchange/Teams/Azure coverage and dashboard inputs using existing profile and
boolean-condition support. Include cost drivers without guessed prices.

Then implement only verified mapping gaps, in a separate small change. Add fixtures
for Minimal/default choices, optional requirements disappearing/reappearing, and
review invalidation after edits. No new expression language or inferred Bicep UI.

**Done:** exact revision and mapping documented; manifest/schema tests agree with
the actual template; admins can see what each optional choice adds. If the template
cannot be represented safely, record the unsupported input and terminal path rather
than labeling it fully guided. A live Maester pilot requires separate resource,
identity, permission, cost, and cleanup authorization.

### P5 — Make management evidence useful without overstating it

**Owner:** Sol medium contract decision → Terra medium implementation → Sol high review.
**Dependency:** P4 mapping; no dependency on live pilot authorization.
**Entry points:** `src-tauri/src/{validation_report,deployment_receipt,models}.rs`,
`src/components/{ProjectValidationReport,ProjectDeploymentReceipt}.svelte`,
`src-tauri/tests/fixtures/`, `src/lib/types.ts`.
Reference: `standards/deployment-receipts.md`, `schemas/deployment-receipt.schema.json`,
`components/powershell/deployment-receipt/`, `tests/DeploymentReceipt.Tests.ps1`.

Start with one proposal defining what can be correlated to project/environment,
target, source, and operation, what is producer-asserted, and what is independently
verified. Prefer optional backward-compatible metadata; explicitly handle missing,
stale, mismatched, malformed, and future-version reports. Do not make legacy reports
look current or use timestamps alone as deployment identity. Keep validation,
resource mutation, delivery, and cleanup distinct.

Only after review, update the reference producer/schema/tests in its own PR, pin
the merged revision, then update the GUI reader/fixtures in a separate PR. Expose
bounded structured next actions with an owner only when the contract can support
them safely; no arbitrary HTML, raw diagnostic text, or followed artifact paths.

**Done:** real producer fixtures round-trip through the reader; old reports remain
readable with honest limitations; mismatches cannot turn the workspace green.
Actual template adoption and observed delivery remain separate evidence gates.

### P6 — Prove recoverable management of the adopted solution

**Owner:** Terra medium; Luna may handle one isolated regression; Sol high review
for operation identity, cancellation, or persistence changes.
**Dependency:** P4; use P5 correlation when available without blocking basic recovery.
**Entry points:** `src/lib/project-state.ts`,
`src/lib/controllers/{deployment-controller,project-operation-controller}.ts`,
`src/App.operation.test.ts`, `src/App.project-template.test.ts`, `src/App.test.ts`,
`src-tauri/src/{azd,command_runner,persistence}.rs`.

Exercise partial preparation, preview failure/cancellation, deployment failure,
app restart, environment switching, unsaved edits, retry, and teardown using
deterministic fixtures and deferred events. Fix demonstrated gaps only. Preserve
the matching operation/session/project/environment identity through every callback.
Show one safe next action; keep logs available without making raw logs the primary
admin experience. Never automatically retry mutation or delete a partial directory.

**Done:** preview cannot become deployment success; unrelated/early events cannot
redirect cancellation or strand retry; reopening does not claim resources from
stale reports; cleanup is not blocked solely by missing deployment configuration.
Existing passing regressions are reused rather than duplicated. Live rerun and
cleanup of Maester remain part of P7, not simulated proof here.

### P7 — Controlled solution pilot and Windows release decision

**Owner:** Sol medium, high for final boundary review; user completes sign-in and
supplies outstanding cloud authorization. **Dependency:** P1–P6 relevant checks.
**Entry points:** Windows acceptance docs, Actions policy, release verifier,
`.github/workflows/release.yml`, the chosen template's reviewed lifecycle hooks.

Execute the approved template's exact journey: select, configure, preview, deploy,
reopen, inspect validation/next actions, rerun where safe, down, independently verify
cleanup including non-ARM objects if applicable. A successful CLI exit alone is
insufficient. Do not repeat the empty-group pilot as a substitute for solution proof.

Select one application revision after local fixes settle. Dispatch signing only
with applicable authorization; verify downloaded and installed payloads before
launch. Repeat affected native/upgrade/runtime gates, record the exact Windows
build, and determine coverage for the approved Windows 11 x64 beta support. Keep the
historical installer limitation explicit. A draft/tag/publication is a separate
user decision after remaining acceptance is met; this plan does not authorize it.

**Done:** an evidence-backed release decision or a precise remaining gate, with
all authorized pilot cleanup verified. No manufactured all-green status.

## Deferred work

Do not add PIM/notification templates, private-host Git authentication, macOS/Linux
packaging, broad component refactors, publisher PKI, telemetry, or new deployment
backends to these milestones. Reconsider after one useful solution completes P7.
An isolated pinned cargo-audit cache is an optional later cost improvement, not a
prerequisite; cache trust must be reviewed and fresh audits retained. Do not rerun
signing merely for documentation or workflow-only planning changes.

## Validation and handoff contract

Use Node 22 and the repository's existing scripts. Frontend behavior changes:
focused Vitest tests plus `npm run check`; native changes: relevant Rust tests,
formatting and Clippy. Before application review, follow Actions policy with local
`npm run verify` and dependency audits at the integrated candidate. Reference
changes use its current documented Pester/schema checks; inspect that repository's
instructions before choosing commands. No automatic hosted development CI.
Documentation-only packets use diff/link/path checks, not full builds. Live UI,
signature, authentication, cloud, and delivery claims require their own evidence.

Use this dispatch packet; include only relevant context:

```text
Task: Pn, one concrete deliverable.
Model/effort: explicit selection from the table.
Baseline: exact commit; owned worktree and allowed files.
Read: relevant entry points and constraints from EXECUTION-PLAN.md.
Change: required behavior and examples; explicit non-goals.
Validate: focused checks and exact success criteria.
Stop: on missing authorization, unresolved contract, or two failed attempts.
Return: commit/diff, changed paths, checks/results, risks, next step.
Do not start the next packet or dispatch hosted/cloud work.
```

The owner maintains a short `docs/EXECUTION-STATUS.md` when execution first resumes:
current packet/status, exact baseline and merged commits, completed checks, pending
input, and the next command/action. Record relevant artifacts/run IDs and cleanup
state without credentials or raw private evidence. Separate code complete from live
acceptance complete. Do not create this status file or mark packets started during
the planning-only turn.

Recommended next prompt, after selecting **Sol / medium**:

> Follow docs/EXECUTION-PLAN.md. Complete P1 only, using Luna for bounded documentation
> work if delegation is worthwhile. Do not start sign-in, cloud deployment, signing,
> or publication. Update the execution status and stop at the milestone.
