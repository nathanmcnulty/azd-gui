# PIM solution planning in the Windows GUI

From Home, choose **Plan PIM solution**. Enter the Graph account, tenant and an
existing public client ID. Select role definition IDs in their appropriate tiers,
authentication profiles and device requirements. Review the configuration, approve
the exact sign-in, then choose **Generate read-only plan**. The screen shows proposed
context, Conditional Access and role-policy changes, warnings and the complete JSON
artifact. This plans deployment of azd-pim; it does not activate the operator's roles.

Changing any field, cancelling or leaving the screen clears the review and result.
Each review has a unique identity; its cancellation cannot close a newer review or
an unrelated wizard's session. A cancelled/failed credential process is never reused.
There is no enforcement action, Azure provisioning, notification configuration,
emergency-group lookup, session revocation or deployment-state adoption in this flow.

After sign-in, **Verified process evidence** shows the observed account, tenant,
client, actual returned grant set, a principal fingerprint, worker/session identity
and read counters. Existing client consent can return more grants than requested;
the requested permission list and the actual grant set are shown separately. A PIM
session permits exactly one verification GET and at most 100 PIM GET attempts.
Trying to probe it again fails before dispatch and revokes the session.

During planning, native code validates metadata-only progress frames from the owned
worker. The screen polls that cached metadata without making another Graph request.
Started/completed counters provide evidence for cancelling an in-flight read. They
describe dispatch attempts and validated completions, not a network packet capture.
Counters and observed identity also accompany the complete plan JSON. **Copy complete
plan JSON** copies it only on request; retain tenant results privately.
An explicit cancellation also offers a separate inactive receipt with the counters
at the cancellation request. It clears the plan and cannot authorize another read.

Failed reads return fixed diagnostic categories for access denial, missing scopes,
identity/grant changes, budgets, redirects or planner rejection. Raw SDK exceptions,
tokens and response bodies are never used as error messages. The last safe failure
receipt may remain visible as inactive historical metadata; it cannot authorize
another read. Disposal clears the matching receipt; a terminal failure requires a
fresh review before another explicit sign-in. Existing template policy/context conflicts remain intentional rejections;
the GUI does not adopt objects or relax configuration to force qualification.

## Pinned planner and process boundary

`src-tauri/pim-template/source.json` pins azd-pim commit
`089a1067c983d490ded667eeb1a9bc959c301d69` and the original module/license byte hashes.
The companion change adds `New-AzdPimReadPlan` to the template's Graph module. It
invokes the existing `New-AzdPimPlan` with no deployment state and an explicit reader,
rejects unsupported extensions/mode, blocks write methods and request bodies, and
clears the reader in `finally`. Its existing CLI/SDK path is unchanged. The GUI embeds
these exact public modules; there is no runtime download or renderer-selected script.
Publish/review the companion PR before publishing the GUI PR that pins it.

The native review digest binds the review ID, identity, full typed configuration,
template commit and permissions. The native session registry checks that binding
before and after the fixed `pimPlan` RPC. The same PowerShell process owns the pinned
planner and managed MSAL credential. Every protected GET silently refreshes that same
account and validates identity, client, resource, expiry and the complete original
grant set before sending that result's token. Tokens remain in managed code.

Only seven registered initial Graph queries are allowed: both privileged-role
classification filters, directory role-policy assignments and policies/rules,
authentication contexts, Conditional Access policies and authentication strengths.
Pagination accepts only an exact link returned by the preceding protected response,
on the same HTTPS host/port/path, preserving non-pagination query parameters. Links
are consumed once. Redirects, arbitrary endpoints, query expansions and write methods
are rejected. Reads are bounded to 100 requests and 16 MiB per owned session, 4 MiB
per response, 30 seconds per read, and 180 seconds per plan. The plan JSON is limited
to 8 MiB; the native response frame is limited to 16 MiB. Session lifetime is 15 minutes.
Large tenants that exceed these bounds fail closed and need a separately reviewed
limit change; the workflow does not silently truncate their plan.

## Required delegated read permissions

The workflow requests exactly these three permissions, without a write permission
or `.default`:

| Permission | Planner reads |
| --- | --- |
| `RoleManagement.Read.Directory` | Role definitions and directory role-policy assignments/policies/rules |
| `Policy.Read.All` | Conditional Access policies and authentication strengths |
| `AuthenticationContext.Read.All` | Authentication context references |

Microsoft documents `RoleManagement.Read.Directory` as sufficient for both
[role definitions](https://learn.microsoft.com/en-us/graph/api/rbacapplication-list-roledefinitions?view=graph-rest-beta)
and [role-management policies](https://learn.microsoft.com/en-us/graph/api/policyroot-list-rolemanagementpolicies?view=graph-rest-1.0),
so an additional role-policy permission is unnecessary. See also the permissions for
[Conditional Access](https://learn.microsoft.com/en-us/graph/api/conditionalaccessroot-list-policies?view=graph-rest-beta),
[authentication strengths](https://learn.microsoft.com/en-us/graph/api/authenticationstrengthroot-list-policies?view=graph-rest-1.0)
and [authentication contexts](https://learn.microsoft.com/en-us/graph/api/conditionalaccessroot-list-authenticationcontextclassreferences?view=graph-rest-1.0).
Delegated permissions do not replace the signed-in user's supported Entra directory
role. New authentication, consent changes and live qualification require separate
operator approval. Never broaden permissions or activate a role automatically after
a rejected read.

## Offline verification

Use the workspace's Windows build environment, Node 22, .NET 8 and PowerShell 7.6.
`npm run check`, `npm test`, `npm run build`, native formatting, Clippy and native tests
cover the app regressions, configuration digests and pinned module hashes.

`tools/Test-GraphProcessAdapter.ps1 -BuildRoot <private-build-directory>` builds only
the test MSAL/network substitutes and runs the real worker integration tests. PIM
cases run the actual pinned planner through Rust, PowerShell and managed HTTP,
including pagination, phishing-resistant/device planning, cancellation, changed
configuration and scoped late disposal. It also preserves the existing Graph tests.

`tools/Test-PimPlanner.ps1 -BuildRoot <private-build-directory> -PlannerRoot <azd-pim-checkout>`
checks the managed reader's negative boundaries, then runs the actual consumer
planner/report writer with offline Graph responses. The managed boundary cases cover
foreign/altered pagination, unissued/consumed links, malformed/deep/oversized JSON,
request/aggregate limits, denial/redirect, insufficient scopes and later-page
credential/grant changes. Neither validator authenticates or contacts Graph.

Live authentication and tenant-specific results are deliberately excluded from
public fixtures, source and PR descriptions. Store operational evidence and exact
live approval packets in the private workspace evidence directory.
