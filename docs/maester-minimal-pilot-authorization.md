# Maester Minimal pilot authorization packet

Prepared 21 September 2026 (America/Los_Angeles). This packet makes the
remaining P7 cloud decision concrete. It records a proposed scope; it does not
authorize or start authentication, deployment, directory mutation, or cleanup.

## Proposed run

| Boundary | Exact proposal |
| --- | --- |
| Template | `nathanmcnulty/azd-maester`, `automation-account`, commit `34f4bac7b69ea2a6746705a91f35ef89ab346034` |
| GUI contract | Bundled `maester-automation` manifest and the **Minimal pilot** recommendation |
| Tenant | `847b5907-ca15-40f4-b171-eb18619dbfab` |
| Subscription | `sub-mvp` (`a80941e8-c2b9-4bc9-83ad-117cc40d0bea`) |
| Environment | `maester-pilot-20260921` |
| Resource group | `rg-azd-gui-maester-pilot-20260921`, created only in the subscription above |
| Location | `eastus2`, the pinned template's declared default |
| Duration | One supervised same-day lifecycle: preview, deploy once, inspect, reopen, then down and independent cleanup verification |
| Spending boundary | Proposed authorization limit: **US$10** of incremental pilot spend. Azure cost data can be delayed, so this is not a guaranteed real-time stop. A dated pre-run estimate and same-day teardown are required. No price or free-tier claim is made. |

The source revision is the one already qualified by
[Maester Automation Account qualification](maester-automation-qualification.md).
It remains a proposal until its required attested reference-component pins are
available and a final source revision containing those pins is reviewed. Before
execution, one focused adoption change must add and verify the signed
`deployment-receipt` and `deployment-validation` patch versions and SHA-256 locks,
update the bundled catalog to the resulting immutable commit, update this packet
and the exact-source qualification to that commit, and rerun the catalog/manifest
and qualification checks. Editing only the commit shown in this document is
insufficient. No branch name or mutable reference is acceptable.

## Selected choices

| Setting | Value | Result |
| --- | --- | --- |
| `INCLUDE_WEB_APP` | `false` | No App Service plan, Web App, Easy Auth application, service principal, delegated grant, or security-group lookup. |
| `INCLUDE_EXCHANGE` | `false` | No `Exchange.ManageAsApp` assignment or Exchange RBAC setup. |
| `INCLUDE_TEAMS` | `false` | No Teams Reader directory-role assignment. |
| `INCLUDE_AZURE` | `false` | No Azure Reader assignments. |
| `PERMISSION_PROFILE` | `Minimal` | The Automation managed identity still receives eight Microsoft Graph application roles listed below. |
| `MAIL_RECIPIENT` | empty | No `Mail.Send` application role and no delivery claim. |
| `AZURE_RBAC_SCOPES` | empty | Irrelevant while Azure checks are disabled. |
| `WEB_APP_SKU` | `F1` | Retained as an unused environment value; no Web App is deployed. |

Do not broaden any choice during the run. A dashboard, Exchange, Teams, Azure
scope, email recipient, Extended profile, different tenant/subscription, or
additional resource group requires a new reviewed authorization.

## Changes this minimal run still makes

The resource group deployment creates an Azure Automation Account with a
system-assigned managed identity, a Standard LRS StorageV2 account, blob
containers and lifecycle policy, a PowerShell 7.4 Automation runtime and module
packages, a runbook, a weekly schedule, role assignments needed for solution
storage, and delete locks. The postprovision hook publishes and starts the
runbook. Package acquisition and the template's intermediate mutable runbook URL
remain supply-chain review items even though postprovision replaces the runbook
with the pinned local script.

The Automation identity receives Storage Blob Data Contributor on the solution
storage account. When the hook can resolve the signed-in Azure CLI user, that user
also receives Storage Blob Data Reader on the storage account. Both assignments
are inside the resource-group lifecycle, but their IDs and principals must still
be captured and independently checked after teardown.

The Automation identity receives these Microsoft Graph **application** roles:

- `Directory.Read.All`
- `DirectoryRecommendations.Read.All`
- `IdentityRiskEvent.Read.All`
- `Policy.Read.All`
- `Policy.Read.ConditionalAccess`
- `RoleManagement.Read.All`
- `Reports.Read.All`
- `UserAuthenticationMethod.Read.All`

The deployment operator needs subscription-scope authority to create the proposed
resource group. Within that group, the operator needs effective Azure resource
deployment and lock authority plus role-assignment write authority for the two
storage roles. Confirm those distinct Azure permissions before mutation; access
to an existing resource group does not prove authority to create this one or to
write role assignments.

For the delegated Microsoft Graph call that assigns application roles to the
managed identity, the signed-in operator needs `AppRoleAssignment.ReadWrite.All`
and `Application.Read.All`, plus a supported Entra role. Microsoft documents
Privileged Role Administrator as the least-privileged built-in role supported for
Microsoft Graph application-role grants. The pinned hooks additionally name
`Directory.Read.All` and `Directory.AccessAsUser.All`; their token check does not
prove that these scopes or corresponding effective privileges are present. Review
the current [Microsoft Graph app-role assignment requirements](https://learn.microsoft.com/graph/api/serviceprincipal-post-approleassignments)
before the run and verify the effective operator role and token permissions before
mutation.

The interactive terminal is the final authority boundary. Device-code
authentication is prohibited; use only the cached broker/WAM session or a normal
browser sign-in.

## Stop conditions

Stop before mutation if any of these conditions occurs:

- the prepared source does not match the final reviewed commit and GUI contract;
- tenant, subscription, environment, resource group, or location differs from
  this packet;
- preview adds an unlisted resource, role, permission, feature, or paid service;
- the signed-in identity or effective write authority is uncertain;
- a runtime dependency resolves to unexpected content or cannot be recorded;
- a dated estimate covering Basic Automation execution, the first runbook job,
  Standard LRS storage and retention, and module acquisition has not been recorded;
- the US$10 authorization limit or same-day teardown window cannot be maintained,
  accounting for delayed Azure cost reporting;
- the GUI loses project, environment, target, source, or operation correlation.

This first pilot authorizes one deployment only. Stop before any second `azd up`
or provision attempt. The pinned preprovision hook can remove the Automation
Account delete lock and delete every live job schedule during redeployment; a
retry therefore requires a new explicit decision after confirming that every
schedule belongs to this pilot.

Do not retry a mutation automatically. Preserve the terminal output and local
operation record, inventory partial resources and directory assignments, then
choose an explicit resume or cleanup action.

## Evidence to retain

Before deployment, retain the final commit, prepared-source digest, manifest
digest, selected values, target IDs, preview, tool versions, and signed-in account
context. After deployment, retain the GUI operation and environment identity,
deployment receipt, validation report, setup summary, ARM deployment result,
resource inventory, Automation identity object ID, its Graph app-role assignment
IDs, storage role-assignment IDs, lock and schedule state, runbook job result, and
observed report blobs. Immediately query and capture the randomly named Storage
Blob Data Reader assignment that the hook may create for the signed-in user,
including its assignment ID and principal ID. A hook or CLI exit code alone is
insufficient.

This profile configures no recipient, so the pilot makes no email-delivery claim.
Validation, report generation, resource deployment, and cleanup remain separate
outcomes.

## Cleanup and independent verification

Run the GUI's reviewed `azd down` path while the same exact project, environment,
and target remain selected. The `predown` hook must run before ARM deletion so it
can remove tracked optional-feature assignments and release resource locks. In
this Minimal profile it does not explicitly delete the eight Graph app-role
assignments. Those assignments should disappear when Azure deletes the Automation
Account's system-assigned identity, so identity deletion and assignment absence
require independent proof. Do not delete the resource group first unless recovery
from a failed down requires it.

After down completes, independently verify:

1. the resource group and its deployments/resources are absent;
2. the Automation managed identity no longer resolves in Entra;
3. no Graph app-role assignment remains for its captured object ID;
4. no Azure role assignment remains for the Automation identity, including its
   Storage Blob Data Contributor assignment;
5. the captured Storage Blob Data Reader assignment for the signed-in user is
   absent;
6. no unexpected application, service principal, delegated grant, directory role,
   Exchange assignment, or broader Azure role was created;
7. the local azd environment and generated output files are retained or removed
   according to the evidence record, without treating local deletion as cloud
   cleanup proof.

If automated cleanup is incomplete, inventory the residue first and remove only
objects tied to the captured pilot identity and operation. Record every manual
action and repeat the independent checks.

## Authorization needed

The live pilot can proceed only after the owner explicitly approves this complete
boundary, including:

- the final immutable replacement for the provisional template commit;
- its bundled catalog revision, exact-source qualification, and signed component
  version/hash locks;
- tenant, subscription, resource group, environment, and `eastus2` location;
- the Minimal profile and its eight Graph application roles;
- subscription-scope resource-group creation authority, resource-group deployment,
  lock and role-assignment authority, the effective Graph token permissions, and
  a supported Entra role for granting application roles;
- creation of the listed ARM resources, locks, the Automation identity's Storage
  Blob Data Contributor assignment, the signed-in user's possible Storage Blob
  Data Reader assignment, schedule, runbook publication, and first run;
- a dated cost estimate and maximum incremental authorization limit of US$10,
  acknowledging delayed cost reporting;
- same-day `azd down`, independent ARM and directory verification, and bounded
  manual cleanup of captured pilot residue if required.

Approval of an earlier resource-group-only skeleton pilot or the Artifact Signing
account does not authorize this solution deployment.
