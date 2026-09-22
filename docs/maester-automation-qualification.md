# Maester Automation Account qualification

This qualification covers the bundled `maester-automation` entry at
`nathanmcnulty/azd-maester` commit
`1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff`, subdirectory
`automation-account`. Its deployed predecessor
`45f839bc2c335cd1c7fa4c40ffe9c6978f715362` completed the separately authorized
[Maester Minimal live pilot](maester-minimal-pilot-20260921.md). The adopted
commit changes only validation-stream presentation: it retains actionable
output, warnings, and errors, summarizes omitted progress/debug noise, and
labels completed jobs with error streams as lifecycle success requiring report
review. Its evidence report also records `validationScope: lifecycle` and
`testOutcomesEvaluated: false` instead of implying that job completion means all
Maester tests passed. It does not change infrastructure, permissions, feature
choices, component locks, or cleanup.

The template uses interactive `preup`, `preprovision`, `postprovision`, and
`predown` PowerShell hooks. Azure Deployment Studio therefore prepares the
pinned source and opens an external terminal. Terminal launch is not deployment
success. The administrator must review the terminal prompts and independently
verify deployment and cleanup.

## Admin choices

| Question | Environment value and guided default | What the exact source does | Authority and cost impact | Evidence and cleanup |
| --- | --- | --- | --- | --- |
| Include secured web dashboard? | `INCLUDE_WEB_APP=false` | When enabled, creates an App Service plan and Linux Web App, an Entra application and service principal, Easy Auth restricted to the selected security group, and an all-principals delegated grant for `openid profile email User.Read`. The Automation identity receives Website Contributor on the Web App. | Adds App Service resources and tenant application, service-principal, and consent writes. The selected App Service SKU is a cost driver. | The Web App, plan, identity role assignment, and locks are ARM resources. Predown attempts to remove the Easy Auth application before resource-group deletion. Verify both ARM removal and absence of the Entra application/service principal. |
| Allowed dashboard security group | `SECURITY_GROUP_OBJECT_ID` is required when the dashboard is enabled | Restricts Easy Auth to members of the selected security-enabled Entra group. The wizard supports explicit ID selection from a bounded name-prefix search. | Group discovery needs effective directory-read authority. Dashboard configuration needs effective `Application.ReadWrite.All`, `Directory.Read.All`, and `DelegatedPermissionGrant.ReadWrite.All` authority; resolving a group by display name also needs `Group.Read.All`. The hook names these scopes but does not request consent or verify them. | The group is not created or deleted. Retain its reviewed object ID and verify the deployed Easy Auth policy references that exact group. |
| Include Exchange checks? | `INCLUDE_EXCHANGE=true` | Adds `Exchange.ManageAsApp` to the Automation identity and attempts to create/link its Exchange service principal and grant `View-Only Configuration`. | Requires Graph app-role assignment authority and Exchange RBAC authority. The local `ExchangeOnlineManagement` module is needed by the setup and cleanup hooks. The Automation runtime package exists even when this choice is off. | Newly created assignment IDs are recorded. Predown removes tracked app-role assignments and attempts Exchange RBAC cleanup. The latter is best effort, so verify it independently. |
| Include Teams checks? | `INCLUDE_TEAMS=true` | Assigns the Entra `Teams Reader` directory role to the Automation identity. The Automation runtime package exists even when this choice is off. | Requires Entra directory role-assignment authority. | Newly created role-assignment IDs are recorded and removed by predown. Verify the role assignment is absent afterward. |
| Include Azure checks? | `INCLUDE_AZURE=true` | Assigns Azure `Reader` to the Automation identity at each requested scope. A blank scope list uses the selected subscription. | Requires role-assignment authority at every scope. A management-group scope reaches all applicable descendant subscriptions and resources. | Newly created assignment IDs are recorded and removed by predown. Verify every requested scope afterward. |
| Azure RBAC scopes | `AZURE_RBAC_SCOPES` is blank by default | Accepts semicolon-separated ARM scope IDs; setup uses the selected subscription when blank. | Scope breadth controls the identity's Azure visibility. It does not create a separate priced resource. | The setup summary and saved assignment IDs provide the cleanup inputs. |
| Mail recipient | `MAIL_RECIPIENT` is blank | A non-empty value adds the Microsoft Graph application role `Mail.Send` to the Automation identity and enables report email. | Adds a tenant-wide application permission. | The role belongs to the resource-group-managed Automation identity. Verify identity removal and absence of its app-role assignments after teardown. |
| Permission profile | `PERMISSION_PROFILE=Extended` | `Minimal` assigns eight baseline Microsoft Graph application roles. `Extended` assigns those eight plus eleven more roles for device management, privileged access, report settings, directory role eligibility, identity security, SharePoint settings, and threat hunting. | Both profiles write directory app-role assignments. Minimal never means zero directory changes. | The setup hook assigns the roles to the Automation managed identity. Verify the identity and assignments are gone after teardown. |
| App Service SKU | `WEB_APP_SKU=F1`, shown only for the dashboard | Sets the App Service plan SKU. | The SKU and region determine dashboard capacity and price. Check current Azure pricing and subscription availability before deployment. | Inspect the deployed plan SKU and verify plan removal during teardown. |

The guided defaults intentionally select Exchange, Teams, Azure, and the
Extended permission profile. The raw template's Bicep and non-interactive hook
fallbacks default the three optional service integrations to `false`. The GUI
writes every guided selection into the azd environment, so the review page is
the authoritative statement of the intended run.

## Baseline changes

Every profile creates an Azure Automation Account with a system-assigned managed
identity, a StorageV2 account, a weekly runbook schedule, storage containers and
lifecycle policy, resource locks, and the Automation runtime. The Automation
identity receives Storage Blob Data Contributor. The signed-in deployment user
may receive Storage Blob Data Reader on the solution storage account.

Baseline setup needs the operator's effective `Application.Read.All`,
`AppRoleAssignment.ReadWrite.All`, `Directory.Read.All`, and
`Directory.AccessAsUser.All` authority so it can assign the selected Graph
application roles to the Automation identity. Enabling Exchange or Teams also
needs `RoleManagement.ReadWrite.Directory`. The helper passes these names to a
cached Azure CLI Graph access check, but that check only obtains a Graph resource
token and reads the organization. It does not request consent or verify that the
token contains the named scopes. These setup permissions are distinct from the
application roles granted to the deployed identity and from the read-only
checks the GUI can perform.

Minimal grants these eight Microsoft Graph application roles:

- `Directory.Read.All`
- `DirectoryRecommendations.Read.All`
- `IdentityRiskEvent.Read.All`
- `Policy.Read.All`
- `Policy.Read.ConditionalAccess`
- `RoleManagement.Read.All`
- `Reports.Read.All`
- `UserAuthenticationMethod.Read.All`

Extended also grants:

- `DeviceManagementConfiguration.Read.All`
- `DeviceManagementServiceConfig.Read.All`
- `DeviceManagementManagedDevices.Read.All`
- `DeviceManagementRBAC.Read.All`
- `PrivilegedAccess.Read.AzureAD`
- `ReportSettings.Read.All`
- `RoleEligibilitySchedule.Read.Directory`
- `SecurityIdentitiesHealth.Read.All`
- `SecurityIdentitiesSensors.Read.All`
- `SharePointTenantSettings.Read.All`
- `ThreatHunting.Read.All`

The principal runtime cost drivers are Azure Automation execution, storage and
retention, and the optional App Service plan. Optional integrations mainly widen
identity authority. This qualification deliberately records no prices because
rates and free grants vary by region, agreement, and time.

## Operations and limits

Redeployment is not read-only. Before ARM deployment, the preprovision hook can
remove the Automation Account delete lock and delete all live job schedules so a
fresh schedule can be deployed. Turning an optional integration off on a later
`azd up` is additive and does not revoke its earlier assignments. The disabled
run also overwrites the saved Exchange, Teams, and Azure assignment-ID lists
with empty lists, so a later predown can no longer remove those assignments by
their tracked IDs. ARM incremental deployment can likewise retain dashboard
resources, and disabling the dashboard does not remove its tenant application.
Run teardown before changing an enabled feature to disabled. If that sequence
was not followed, independently discover and remove every residual resource,
application, service principal, consent grant, and role assignment.

Postprovision publishes the local runbook, starts live runbook validation, and
writes `outputs/<environment>-setup-summary.md`,
`reports/deployment-validation.json`, and
`reports/deployment-receipt.json`. The template vendors the signed and attested
`deployment-validation@1.1.1` and `deployment-receipt@0.2.1` components from
`azd-reference` merge `0c96cc89c554ffc3b3ca82ceda12da6591e816c1` with exact
SHA-256 locks. Its repository validation resolves every hook import to the
locked `scripts/vendor` location and checks both evidence schemas offline.

The interactive-terminal route does not receive a GUI-issued lifecycle
operation ID. Its first pilot therefore produces compatible unbound 1.0
artifacts, which the GUI presents as producer assertions rather than current
correlated evidence. The receipt binding is available only when this template
later runs through a native GUI `up`, `provision`, or `deploy` operation with a
complete injected context; the validation report remains unbound because the
postprovision validation is part of deployment rather than a separate GUI
`validate` operation.

Runtime reports are stored under
`archive/` and `latest/`; archive blobs move to Cold tier after 180 days and are
deleted after 365 days. A successful hook or CLI exit is not proof that reports
were delivered or that tenant-level cleanup completed.

The pinned repository revision does not pin all runtime inputs. PowerShell
Gallery package links omit exact versions, and the Bicep runbook resource points
to the Maester repository's mutable `main` branch before postprovision replaces
the draft with the pinned local script. Treat package acquisition and any
intermediate remote content as supply-chain review items for a live pilot.

The contract cannot prove the operator's write, consent, directory-role,
Exchange, or Azure RBAC authority with its read-only checks. It also cannot
preflight every PowerShell module imported by the interactive hooks. These
remain explicit terminal gates rather than fully guided or automatically
approved behavior.

## Pilot evidence checklist

A separately authorized pilot should retain the selected values and exact
revision, setup summary, ARM resource inventory, Automation identity Graph app
roles, Entra and Azure role assignments, Exchange assignment state, Easy Auth
application/service principal and delegated grant, schedule and lock state, and
observed report/runbook result. Teardown must run `azd down`, then independently
check the resource group, Entra objects, Graph app-role assignments, Teams role,
Exchange RBAC and app role, Azure role assignments at every requested scope, and
the local azd environment/output files.
