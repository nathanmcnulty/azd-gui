# Backlog: nathanmcnulty/azd-gui

> Generated from `docs/backlog.json`. Edit the JSON source and regenerate this file.
> Standard: [azd agent backlog standard](https://github.com/nathanmcnulty/azd-reference/blob/main/standards/agent-backlogs.md). This link is review guidance, not a runtime dependency.

- **Schema version:** 1.0.0
- **Repository:** nathanmcnulty/azd-gui
- **Source revision:** `24b2f14754e147ff9d58e357bb1cafb3141509fa`
- **Captured:** 2026-11-15
- **Items:** 17

## GUI-001: Reconcile this backlog with current source and active work

- **Kind:** discovery
- **Priority:** P1
- **Status:** done
- **Wave:** 0
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Plans and implementation evidence are spread across files; the captured source can change while other tasks work.

**Scope:**

- docs/backlog.json
- docs/backlog.md
- Existing roadmap, execution status, open issues and pull requests &lpar;read-only&rpar;

**Acceptance:**

- Classify each candidate as implemented, still open, superseded or awaiting evidence; retain source links and reasons.
- Inspect dirty state, remotes, worktrees and local environment presence without reading secrets; avoid duplicate work with active owners.
- Resolve the actual offline validation commands and record exact current default-branch/working-tree provenance; do not copy historical live passes to newer code.

**Validation:**

- git status --short
- git remote -v
- git worktree list --porcelain
- Read the applicable instructions and validation workflow; read gh issue list and gh pr list for the named repository using nathanmcnulty. Do not create or modify issues/PRs.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- README.md
- docs/EXECUTION-CONTINUATION-20260927.md
- docs/EXECUTION-PLAN.md
- docs/EXECUTION-STATUS.md
- docs/windows-readiness-roadmap.md
- https&colon;//github.com/nathanmcnulty/azd-gui/pull/134
- https&colon;//github.com/nathanmcnulty/azd-gui/pull/133

**Evidence:**

- Reconciled reviewed tracking tip e3d92a66f4f54796b0baf46585d1d109c9d2e4bd against freshly fetched origin/main aebed2dea6fef6f31615653009412d35e0e3a8ae in a clean owned worktree. No dirty canonical bytes, source files or unrelated branch history were copied.
- Current main adds pull request &num;142, which closes issues &num;140 and &num;141. Issues &num;5, &num;11, &num;41, &num;42, &num;55-&num;59 and &num;143 remain open and proposed; issue &num;143 directly tracks the exact-candidate Windows acceptance gate in GUI-002, while open dependency pull requests &num;133 and &num;134 do not overlap these tracking paths.
- Items GUI-002, GUI-003, GUI-004, GUI-005, GUI-006, GUI-007, GUI-008, GUI-009, GUI-010, GUI-011, GUI-012, GUI-013, GUI-014 remain proposed because their existing acceptance gates are not satisfied by the current source changes alone.
- Exact current-source offline validation passed&colon; npm ci reported zero vulnerabilities; Svelte check reported zero errors/warnings; Vitest passed 143/143; Vite build and cargo fmt/clippy passed; native Rust tests passed 166 with three explicitly ignored integration tests. The first native compile exhausted the E&colon; drive in an ignored target cache; the clean rerun used a private C&colon; Cargo target and passed without changing repository source.

**Review and authorization note:**

Review GUI-001 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-015: Redact credential query values before streaming tool output to the renderer

- **Kind:** discovery
- **Priority:** P1
- **Status:** done
- **Wave:** 0
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

The report captured on 2026-10-03 is closed after the reviewed fix merged; this record preserves the original trigger and validation boundary.

**Scope:**

- Linked issue and current source &lpar;read-only&rpar;
- Repository-local backlog evidence

**Acceptance:**

- Read the linked issue and current default branch; classify the exact defect, current owner and evidence gap.
- Record a current PR or verified resolution before selecting any implementation; preserve broader feature and live acceptance gates.

**Validation:**

- Read current issue and PR state using nathanmcnulty; do not modify or close issues during reconciliation.
- Inspect dirty state and worktrees; resolve the exact current revision and relevant offline commands before implementation.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/141

**Evidence:**

- GitHub issue &num;141 is closed by merged pull request &num;142; current origin/main aebed2dea6fef6f31615653009412d35e0e3a8ae contains the reviewed fix.
- The fix is source and regression evidence only; no signed-candidate acceptance, release, deployment, tenant mutation or human-visible result is claimed.

**Review and authorization note:**

Review GUI-015 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-016: Reject Windows rooted and drive-relative catalog subdirectories

- **Kind:** discovery
- **Priority:** P1
- **Status:** done
- **Wave:** 0
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

The report captured on 2026-10-03 is closed after the reviewed fix merged; this record preserves the original trigger and validation boundary.

**Scope:**

- Linked issue and current source &lpar;read-only&rpar;
- Repository-local backlog evidence

**Acceptance:**

- Read the linked issue and current default branch; classify the exact defect, current owner and evidence gap.
- Record a current PR or verified resolution before selecting any implementation; preserve broader feature and live acceptance gates.

**Validation:**

- Read current issue and PR state using nathanmcnulty; do not modify or close issues during reconciliation.
- Inspect dirty state and worktrees; resolve the exact current revision and relevant offline commands before implementation.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/140

**Evidence:**

- GitHub issue &num;140 is closed by merged pull request &num;142; current origin/main aebed2dea6fef6f31615653009412d35e0e3a8ae contains the reviewed fix.
- The fix is source and regression evidence only; no signed-candidate acceptance, release, deployment, tenant mutation or human-visible result is claimed.

**Review and authorization note:**

Review GUI-016 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-002: Complete exact-candidate Windows 11 human acceptance

- **Kind:** verification
- **Priority:** P1
- **Status:** proposed
- **Wave:** 2
- **Authorization:** azure-deployment
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

The active continuation retains release no-go for clean-profile sign-in, visual wizard and lifecycle gates.

**Scope:**

- docs/windows-acceptance-status.md
- docs/EXECUTION-STATUS.md
- docs/EXECUTION-CONTINUATION-20260927.md

**Acceptance:**

- Use the exact current signed source and installed executable hashes in a normal standard-user Windows 11 desktop.
- Record browser/WAM identity, unavailable-target rejection, visual wizard, cancel/recovery, draft reopening and secret-marker checks.
- Update each gate with current evidence; historical Sandbox/CLI proofs do not become current GUI passes.

**Validation:**

- Follow section A of docs/EXECUTION-CONTINUATION-20260927.md with the maintainer in the prepared full desktop.
- Separate read-only visual/auth checks from the separately authorized deploy/verify/down lifecycle and cleanup.
- Run npm run verify only for affected code; it is not human acceptance.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- docs/EXECUTION-CONTINUATION-20260927.md
- https&colon;//github.com/nathanmcnulty/azd-gui/issues/143

**Evidence:**

- Open issue &num;143 records the still-required exact-candidate Windows acceptance after human sign-in; current offline source validation does not satisfy that human gate.

**Review and authorization note:**

Review GUI-002 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-017: Design a target-bound regional quota preflight for deployments

- **Kind:** discovery
- **Priority:** P1
- **Status:** proposed
- **Wave:** 2
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

The Maester preprovision hook warns from the count of regional Automation Accounts, but a count cannot show the subscription&#39;s effective per-region limit or remaining capacity. azd-gui has no authoritative capacity result before a deployment action allowed by the active project contract, so ARM can still reject account creation after preparation.

**Scope:**

- Investigate supported read-only data sources for Automation Account usage and quota by subscription and region, including their permission and provider prerequisites. Microsoft&#39;s documented portal flow shows regional Automation Account usage and limits; the Quota REST API&#39;s documented provider set does not include Automation and its prerequisites include Microsoft.Quota registration and Quota Request Operator. The stable Automation ARM spec version 2024-10-23 defines account listing and an account-scoped Usages route, but no subscription/region account-quota operation; confirm whether any other supported endpoint can answer the pre-create question.
- Design a native, host-owned capacity-check contract bound to the exact cloud, subscription, region, resource type, source and check time; do not accept arbitrary template scripts or URLs.
- Specify clear pre-deployment behavior for available, exhausted, unknown, stale, forbidden, timed-out, cancelled and retried checks, including plain-language next steps and invalidation when the bound target or configuration changes.
- Respect the active project contract&colon; do not add or restore a blocked deployment route, copyable reviewed command, arbitrary command, or terminal handoff as part of quota discovery.
- Keep the first supported use case to Azure Automation Account regional quota; do not imply that other resource types or all Azure capacity are covered.

**Acceptance:**

- Document whether a supported read-only API exposes Automation Account per-region usage and limit; identify exact permissions and provider-registration requirements, and distinguish a portal view from a programmatic API.
- Define target-bound available, exhausted and unknown results with evidence source and check time; existing-account counts and saved reports are not authoritative quota proof.
- For each deployment action already allowed by the active project contract, confirmed exhaustion blocks that action; unavailable or stale data remains visibly unknown and points to the documented manual quota view. Any override must be a separate explicit acknowledgement bound to the current check. This discovery must not create a deployment path, terminal handoff, or copyable reviewed command where the project contract blocks one.
- Keep quota reads read-only. Do not register providers, request quota changes, add scopes/consent, switch target, or deploy as part of this discovery. Record that a quota result is not a capacity reservation and ARM may still reject a later create.
- Define deterministic fixtures for sufficient, exhausted, unsupported/unknown, permission denied, stale target, timeout, cancellation and retry; no live tenant mutation.

**Validation:**

- Review Microsoft Learn documentation for Azure Automation quotas, Azure Quota REST provider support, and the Azure portal Quotas view; inspect the stable Automation ARM specification and record API version, permissions and limitations.
- Review current native permission-preflight and terminal-handoff contracts before selecting the host-owned integration point.
- Use deterministic fixtures for every documented result and invalidation case; keep live quota requests, quota increases and deployment outside the item.
- Validate docs/backlog.json and regenerate/check docs/backlog.md with the checked azd-reference backlog tooling.

**Dependencies:**

- _none_

**Components:**

- deployment preflight
- terminal handoff

**Sources:**

- docs/maester-minimal-pilot-20260921.md
- docs/maester-automation-qualification.md
- docs/windows-readiness-roadmap.md
- src-tauri/src/requirements.rs
- src-tauri/src/permissions.rs
- src-tauri/src/terminal.rs
- https&colon;//learn.microsoft.com/en-us/azure/automation/automation-limits-quotas
- https&colon;//learn.microsoft.com/en-us/rest/api/quota/
- https&colon;//learn.microsoft.com/en-us/rest/api/automation/usages/list-by-automation-account?view=rest-automation-2024-10-23
- https&colon;//github.com/Azure/azure-rest-api-specs/blob/main/specification/automation/resource-manager/Microsoft.Automation/Automation/stable/2024-10-23/openapi.json
- https&colon;//learn.microsoft.com/en-us/azure/quotas/view-quotas

**Evidence:**

- Initial documentation review&colon; Azure Automation quota guidance describes current regional Automation Account usage and limits in the portal; Microsoft&#39;s Quota REST API documents a provider set that does not include Automation and requires provider registration plus the Quota Request Operator role; the documented Automation Usages REST endpoint requires an existing account. A read-only review of stable Automation ARM specification version 2024-10-23 found only an account-scoped Usages route, not a subscription/region account-quota operation.

**Review and authorization note:**

Review GUI-017 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-003: Design explicit provider account transitions after the multi-account trial

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

The first two account-choice packets are merged; reconnect/selection should follow observed Windows provider limitations.

**Scope:**

- docs/account-selection-design.md
- src/
- src-tauri/
- tests/

**Acceptance:**

- Use the real multi-account trial to choose bounded explicit reconnect paths for each provider.
- Changing identity/target invalidates readiness and permission proof; no parsing token caches or silent CLI defaults.
- Tests prove wrong account, unavailable target, expiry and cancellation remain fail-closed; human trial remains separate.

**Validation:**

- npm run verify
- Review the current Windows gate evidence and account-selection design before proposing account-transition code.

**Dependencies:**

- nathanmcnulty/azd-gui&colon;GUI-002

**Components:**

- _none_

**Sources:**

- docs/EXECUTION-CONTINUATION-20260927.md
- docs/account-selection-design.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-003 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-004: Improve optional-feature recommendations using proven permissions and receipt state

- **Kind:** feature
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

The wizard should connect optional feature benefit with exact incremental permissions and actual deployment evidence.

**Scope:**

- src/
- src-tauri/
- docs/
- tests/

**Acceptance:**

- Show per-feature permission deltas and missing requirement gaps without treating catalog metadata as authorization.
- Disabled features remain optional and display-only source data stays inert.
- Candidate receipt evidence remains bound to source/target and never implies human-visible endpoint success.

**Validation:**

- npm run verify
- Use fixtures for absent/unavailable permissions, changed source revision and stale receipts.

**Dependencies:**

- _none_

**Components:**

- deployment-receipt
- deployment-validation

**Sources:**

- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-004 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-006: Support tenant-object ownership preflight and verified adoption

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/59
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-006 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-007: Surface vendored component provenance, drift, and update status

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/58
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-007 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-008: Publish the manifest schema at a stable anonymously accessible URL

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/57
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-008 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-009: Support conditional acknowledgements that must be accepted before deployment

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/56
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-009 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-010: Support structured post-deployment reports and verification workflows

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/55
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-010 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-011: Support conditional connections, scopes, prerequisites, and permission checks

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/42
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-011 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-012: Support trusted dynamic option providers for tenant-backed template inputs

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/41
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-012 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-013: Track upstream glib security fix in the Tauri Linux stack

- **Kind:** maintenance
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/11
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-013 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-014: Evaluate remote environment backends and optional ADE compatibility

- **Kind:** discovery
- **Priority:** P2
- **Status:** proposed
- **Wave:** 3
- **Authorization:** local-only
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Open GitHub report captured 2026-10-03. Reproduce against the current source and reconcile active PRs before changing code; the issue remains the detailed trigger/evidence reference.

**Scope:**

- Paths and trigger cited in the linked issue
- Focused offline regression tests
- docs/

**Acceptance:**

- Classify the report as still reproducible, already fixed, superseded or requiring live evidence; record the exact current revision.
- For a reproducible defect, demonstrate the linked trigger with an offline regression and apply the smallest fix preserving tenant/target/ownership and failure semantics.
- For a feature, produce a bounded design with compatibility, optional permissions, acceptance and rollout gates before implementation; no live mutation or automatic issue closure.

**Validation:**

- Read the issue body and current source/PRs; capture the exact reproduction and existing registered offline validation command.
- Use deterministic fixtures for the described trigger and negative boundary; retain current-source results. Do not rerun production or tenant operations to reproduce it.

**Dependencies:**

- _none_

**Components:**

- _none_

**Sources:**

- https&colon;//github.com/nathanmcnulty/azd-gui/issues/5
- README.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-014 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.

## GUI-005: Prepare the exact-candidate release checkpoint

- **Kind:** verification
- **Priority:** P1
- **Status:** proposed
- **Wave:** 4
- **Authorization:** publication
- **Blocker:** _none_
- **Claim:** _none_

**Problem:**

Private signing and hosted tests do not authorize a public beta release.

**Scope:**

- docs/EXECUTION-STATUS.md
- docs/windows-acceptance-status.md
- Release evidence

**Acceptance:**

- All required current-candidate gates and any explicitly accepted exceptions are recorded.
- Separately authorize tag/draft/publication and verify downloaded assets plus installed payload.
- Create a new signed candidate only when application/packaging/workflow changes require it; preserve historical invalid-upgrade evidence.

**Validation:**

- Use the offline commands in the registered validation workflow; record the exact commands, revision and results before implementation is complete.
- After separate authorization, retain redacted exact-target live evidence and cleanup results outside public Git. Do not execute live operations from this backlog alone.

**Dependencies:**

- nathanmcnulty/azd-gui&colon;GUI-002

**Components:**

- _none_

**Sources:**

- docs/EXECUTION-CONTINUATION-20260927.md

**Evidence:**

- _none_

**Review and authorization note:**

Review GUI-005 against the current repository state. Its status or authorization class is not eligible for an actionable generated handoff. Do not claim or execute it without explicit selection, satisfied dependencies, and every required authorization. Never interpret this generated view as approval.
