# Maester Minimal live pilot

Pilot date: 21 September 2026 (America/Los_Angeles).

## Result

The authorized Maester Automation Account pilot completed its full lifecycle at
source `45f839bc2c335cd1c7fa4c40ffe9c6978f715362`. The first viable region was
`westus2`: preflight stopped before deployment in `eastus2` because the
subscription already had two Automation Accounts there. Both preview-created
empty resource groups and the deployed pilot resource group were removed.

The deployment used the approved Minimal profile with the dashboard, Exchange,
Teams, Azure checks, and mail delivery disabled. `azd up` completed in 11 minutes
36 seconds. The runbook reached `Completed`, generated a compressed archive and
`latest.html`, and the template emitted a deployment validation report with four
passing lifecycle checks and no warnings or failures. This proves provisioning,
managed-identity setup, runbook execution, report upload, evidence production,
and teardown. It does not prove that every Maester test passed: the Minimal
profile intentionally lacks permissions used by the broader Maester suite, and
the runbook reported missing-permission warnings and `Forbidden` results for
tests outside that profile.

## Exact boundary

| Item | Observed value |
| --- | --- |
| Source | `nathanmcnulty/azd-maester` commit `45f839bc2c335cd1c7fa4c40ffe9c6978f715362` |
| Environment | `maester-pilot-20260921` |
| Tenant | `847b5907-ca15-40f4-b171-eb18619dbfab` |
| Subscription | `a80941e8-c2b9-4bc9-83ad-117cc40d0bea` |
| Deployment region | `westus2` |
| Resource group | `rg-azd-gui-maester-pilot-20260921` |
| Automation Account | `aa-maester-pilot-20260921` |
| Storage Account | `stmaesterdxlyho7uumrq6` |
| Permission profile | `Minimal` |
| Optional components | all disabled |
| Validation job | `df5d8fc5-f7be-4a06-9c08-7e6425552345`, `Completed` |
| Incremental-spend limit | US$10; resources were removed the same day, and delayed Azure cost data was not treated as a real-time meter |

The Automation managed identity received exactly the eight approved Microsoft
Graph application roles: `Directory.Read.All`,
`DirectoryRecommendations.Read.All`, `IdentityRiskEvent.Read.All`,
`Policy.Read.All`, `Policy.Read.ConditionalAccess`, `Reports.Read.All`,
`RoleManagement.Read.All`, and `UserAuthenticationMethod.Read.All`. It received
`Storage Blob Data Contributor` on the pilot storage account. The signed-in
operator received `Storage Blob Data Reader` on that account so the deployment
could verify the report artifacts. Before teardown, independent queries found no
application object, delegated grant, directory-role assignment, Web App, or
other optional component owned by the pilot.

## Admin-experience findings

- Regional Automation Account quota must be checked before deployment. The
  template's preprovision check stopped safely in `eastus2`; selecting `westus2`
  allowed the authorized run to continue.
- `azd provision --preview` created the otherwise empty target resource group in
  both regions. The GUI must continue to classify preview as possibly modifying
  state and name the cleanup responsibility.
- ARM validation emitted reserved-word warnings for the legitimate PowerShell
  package names `Microsoft.Graph.Authentication` and `MicrosoftTeams`. Azure
  accepted both names. The warning needs concise product guidance so an admin
  can distinguish the known package-name warning from an actual validation
  failure.
- Validation replayed hundreds of low-value Automation `Progress` streams with
  one REST request per stream after the job had completed. The template should
  summarize progress and retain `Output`, `Warning`, and `Error` records so the
  wizard reaches its result promptly.
- The template's passing runtime check means that the job completed and report
  artifacts were written. The admin experience must label this as lifecycle
  validation rather than representing it as a clean Maester test result.

## Cleanup proof

`azd down --force --purge` removed both resource locks and deleted the resource
group in 1 minute 19 seconds. Independent Azure and Microsoft Graph checks then
confirmed:

- the resource group, Automation Account, Storage Account, and pilot RBAC
  assignments were absent;
- the managed-identity service principal was absent;
- every captured Microsoft Graph app-role-assignment ID returned `notFound`;
- no application object, delegated grant, or directory-role assignment had
  existed for the pilot identity.

Private raw evidence is retained outside the repository under
`C:\Users\NathanMcNulty\.codex\artifacts\azd-gui-maester-pilot-20260921`.
The repository records only bounded identifiers and conclusions; it does not
publish tenant reports, tokens, or CLI caches.
