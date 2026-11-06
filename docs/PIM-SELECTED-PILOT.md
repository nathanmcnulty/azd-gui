# Selected-role core PIM pilot

The PIM page continues from the named role picker into a project-bound pilot.
It uses the pinned azd-pim planner, `Invoke-AzdPimApply`, ownership state and
checkpoint recovery. It does not invoke project hooks, deploy Azure resources,
activate/assign roles, enable notifications/session revocation or expand to all
roles. Packaging and broad rollout remain separate.

## Administrator workflow

1. Review the account, tenant and existing public client. Sign in to the read-only
   planner, refresh the complete role list, select named roles and configure MFA
   or phishing-resistant authentication and device requirements. Empty tiers
   manage no roles.
2. Select a registered PIM project, or register a dedicated private workspace
   directory using **Register pilot workspace**. Choose a named environment.
   This local registration runs no template or cloud operation; generic project
   lifecycle actions remain guarded. External reviewed PIM projects are identified
   by the exact repository, not their display name.
3. **Review sign-in and recheck pilot** requests a separate process-owned session
   with azd-pim’s core scopes: `AuthenticationContext.ReadWrite.All`,
   `Policy.Read.All`, `Policy.ReadWrite.ConditionalAccess`,
   `RoleManagement.Read.Directory` and `RoleManagementPolicy.ReadWrite.Directory`. Existing consent is required.
   Stop if new consent is requested. The sign-in dialog is not application approval.
4. Inspect each exact context, CA policy and role policy ID, current object and
   proposed body. Existing-name/role-context conflicts block planning by default.
   The existing preview checkbox permits proposals for those objects; it creates
   no ownership. Duplicate names remain blocked and need administrator resolution.
   Legacy activation MFA removal shows the existing/remaining requirements.
5. Check the explicit approval box, type the tenant ID and choose application.
   The native confirmation identifies the project/environment, identity and exact
   review digest. Before and after that dialog the worker re-reads/replans. Any
   changed object/property, configuration, principal, tenant, project contract or
   ownership checkpoint invalidates the review. Approval is consumed once.
6. Inspect the attempt receipt and durable operation results. A completed engine
   run is **applied, needs verification**, not deployment/activation proof. Run a
   read-only recheck. Then perform the manual activation checks shown in the UI
   for a designated eligible test administrator and each selected tier.

## Recovery and storage

Under `.azure/<environment>/`, the worker reads/writes the existing
`azd-pim-state.json`. It records an attempt before its first write, saves every
engine checkpoint and retains both a latest `azd-pim-gui-receipt.json` and
uniquely identified `azd-pim-gui-<attempt>.json` receipts. A saved
`azd-pim-gui-draft.json` restores configuration/identity only, including after a failed first inventory read. Each application also retains its exact reviewed plan/state in `azd-pim-gui-<attempt>-review.json`. The latest `azd-pim-gui-review.json` contains the last complete review. Its plan is
historical data; no approval/session survives restart.

Cancellation revokes the worker and discards late replies. It can interrupt a
write after Graph committed it. Keep the checkpoint, temporary files and receipt;
reconnect and recheck before approving a new attempt. Read-only reconciliation
proposes the engine's recovery actions without clearing ownership intents.
Explicit application checkpoints recovered operations and applies only the
reviewed remainder. An unresolved operation outside a narrowed selection blocks
application until the original role/tier/controls are restored and rechecked.
There is no automatic replay or automatic rollback.

The environment file lock permits one GUI pilot worker per project environment,
including across app processes. Do not run CLI deployment or another tool against
the same environment concurrently; those tools do not participate in this lock.
Paths must belong to a registered project and reject existing aliases/reparse
points. Store these private files outside public source control.

## Verification boundaries

- Every HTTP operation silently refreshes and validates the same principal,
  tenant, client, resource and actual grant set. Tokens stay in the owning process.
- The adapter admits only registered reads and the core engine's context/CA/PIM
  write routes while application is armed. It never retries a write. Lost responses,
  timeouts and server errors preserve uncertainty; definite rejection permits the
  engine's existing legacy-MFA compensation.
- Graph does not make this multi-object operation transactional. External changes
  after the final comparison can still race writes. Recheck and audit are required.
- An all-`none` recheck establishes only the planner-managed subset. Full live CA
  bodies remain visible: inspect additional conditions/exclusions/controls and test
  activation behavior. Pending intents prevent a matching-settings indication.
- Uncertain server-assigned CA creation requires one unique same-name policy
  matching the complete recorded writable intent, with bounded response metadata
  and empty defaults. Unknown properties or conservative normalization differences
  require manual review; no broad adoption fallback occurs.
- The inherited 100-read/16 MiB/session and 15-minute limits remain. A long pilot
  may require reconnection and a fresh review. Role classification uses beta APIs.
- Removing a selected role later does not detach contexts or restore old settings.

## Reports and validation

The existing deployment-receipt/validation UI distinguishes recorded actions from
verification. This pilot follows that distinction and reuses the status-pill UI;
its engine checkpoint is shown directly because it carries pending-write recovery
information that the generic sanitized receipt does not contain. Open provenance
PR #164 and declared-report viewer PR #167 were inspected. They remain independent
drafts; this change adds no competing general report browser or provenance reader.

Run `npm run ci`, native formatting/Clippy/tests, and
`tools/Test-PimPilot.ps1 -BuildRoot <private-evidence-directory>` on Windows with
PowerShell 7.6+, .NET 8 and Rust installed. The pilot tests exercise the real
native/PowerShell/managed boundary and pinned engine with offline mutable HTTP,
including conflicts, stale approvals, committed-but-lost writes, cancellation,
MFA restoration and restart reconciliation. Ordinary native tests do not run
these explicitly ignored integration tests unless selected with `--include-ignored`.

For a live pilot, first review a private packet identifying the exact candidate,
identity/client, project/environment, selected roles, controls, scopes and allowed
read operations. Generate a fresh live review, resolve conflicts, then separately
approve that exact application digest. Neither this implementation nor a cached
packet authorizes new consent or tenant mutation.
