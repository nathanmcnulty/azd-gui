# Catalog, project recovery and verification

This milestone supports two complete paths: the selected-role native PIM pilot
and the pinned Azure deployment-check example. It reuses the existing planner,
application/checkpoint engine, operation manager, evidence contract and report
views. Portfolio deployment/inventory, general script execution, broad PIM
rollout and automatic component updates remain outside this milestone.

## Selected-role PIM

Choose **Selected-role PIM pilot** in Catalog, or open a saved PIM project from
Home or Projects. The bundled entry routes to the native workflow; its Graph
metadata keeps generic azd preparation and lifecycle hooks blocked. A dedicated
pilot workspace needs no azure.yaml and opening it never invokes azd or Graph.
Existing reviewed projects from the exact azd-pim repository use the same native
experience and pinned embedded engine; their project revision is retained as
source context, not permission to execute their scripts.

Register a private workspace, choose its environment, configure identity and
selected-role controls, and **Save pilot configuration locally**. No sign-in is
needed to save, restore or inspect existing records. Environment selection is
persisted locally. The overview identifies saved drafts, attempt counts, latest
application status, pending writes and recovery guidance. Sensitive receipt and
checkpoint contents require explicit reveal and disappear on context changes.
Malformed, unsafe or oversized records are unavailable, never inferred clean.

Restoring copies identity and configuration only. It restores no session,
approval or live verification. Review the identity, then explicitly choose
**Review sign-in and recheck pilot**. Existing consent is required; stop on any
new consent prompt. The session requests the existing core pilot permissions,
but its recheck uses only registered reads. **Read-only recheck** reruns that
same supported planner in the owned session. A matching result covers only the
planner-managed subset and no pending intents. Application is a separate exact,
single-use approval; activation tests remain a separate administrator task.

After cancellation, authentication loss or partial application, preserve every
checkpoint and attempt receipt, restore the original selected roles and controls,
reconnect and recheck before approving any remainder. Absence after an uncertain
create never permits replay. Saved reports are historical evidence only.

## Azure deployment check

Configure the existing Catalog template, deploy only under its applicable
approval, then reopen its project overview. **Run read-only validation** invokes
the exact pinned example's validation engine and three checks: project files,
existing Azure CLI tenant/subscription context, and resource group read access.
Opening this overview does not refresh live resource state. It retains the
project’s saved environment even when the external azd default changes; choosing
another target is explicit. Validation performs no sign-in, delivery probe,
deployment or cleanup. The target is the
saved environment's tenant, subscription and resource group. A private local
reports/deployment-validation.json records the outcome using the existing v1.1
evidence contract and operation history.

Only the exact supported repository/revision/subdirectory is admitted natively.
Scripts are embedded from that revision, never loaded from the project. The host
adapts their root to the registered workspace, requires the observed Azure CLI
cloud to be public Azure, and adds the existing operation
correlation envelope; the pinned vendored files and hashes remain unchanged.
Windows directory handles pin the project/report ancestry during validation.
A handle-relative, delete-on-close guard keeps the final directory nonempty while
writer-compatible sharing permits atomic report replacement; the original checked
handles remain held throughout that transition. Saved PIM reads traverse native
Windows child handles rather than resolving child paths again. The Windows fixture
tests an actual empty-directory reparse conversion, rejection under the guard,
rename/delete rejection, existing-report replacement and automatic guard cleanup.
The normal operation manager supervises and cancels the child process tree.

The existing validation and receipt views show historical records, even when
source/environment correlation matches. Current-session validation is displayed
separately. Account, environment, source and configuration changes invalidate
that session result and hide/reload report content. Context is observed when
refreshed; an external account/configuration change cannot be detected before
the app refreshes or runs a new check. Failed/cancelled checks can be retried
explicitly. Old completion replies cannot validate a different project context.

## Reconciliation and validation

PR #164 (a7fdd02) and #167 (1f269d1) were inspected as independent drafts. This
milestone reconciles #164's pinned-handle local-reader approach for private PIM
records and #167's explicit reveal/context reset behavior. It does not merge
either draft or create a competing general provenance/report browser. The
existing sanitized deployment receipt and validation views remain the common
report surface. Report text and source strings remain inert.

Run npm run verify, then tools/Test-AdminJourneys.ps1 -BuildRoot <private-dir>
with Node 22, PowerShell 7.6+, .NET 8 and Rust. The latter runs real frontend
fixtures, the pinned validation scripts through the native process boundary with
a fake Azure CLI, and the existing real PIM worker/engine mutable HTTP fixtures.
These tests use no live sign-in or tenant changes. Live account access,
Conditional Access propagation and actual administrator activation behavior
remain manual qualification checks.

A live packet must bind the exact rebuilt executable hash, Git commit/tree,
embedded engine source, selected identity/client and project/environment. Any
code or recovery fix invalidates an earlier packet. Rebuild and regenerate it
before requesting live sign-in/read-only checks. Tenant changes require their
own fresh exact-plan approval; consent, deployment, signing and releases require
their applicable authorization.
