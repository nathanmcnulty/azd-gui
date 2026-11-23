# Automation Account regional quota discovery

The Azure portal provides a manual check for the number of Automation Accounts
in a subscription and region. In **Quotas → My quotas**, select **Automation
accounts**, the intended subscription, and the intended location, then read the
current usage and limit. This is a read-only operator step. Use the current
account's existing permissions for the view. Microsoft's Automation guidance
lists Contributor on an active account for **requesting changes** and gives no
separate minimum role for viewing this quota. [Microsoft's Automation quota guidance](https://learn.microsoft.com/en-us/azure/automation/automation-limits-quotas)

## API decision

The reviewed public surfaces give the GUI no supported pre-create lookup for
both usage and the effective subscription/region limit.

| Surface | Evidence |
| --- | --- |
| Automation ARM, stable 2024-10-23 | Account listing is inventory. The usage operation requires an existing account and the `Microsoft.Automation/automationAccounts/usages/read` action. The specification has no regional account-quota route. [ARM specification](https://github.com/Azure/azure-rest-api-specs/blob/main/specification/automation/resource-manager/Microsoft.Automation/Automation/stable/2024-10-23/openapi.json) · [Provider operations](https://learn.microsoft.com/en-us/rest/api/automation/operations/list?view=rest-automation-2024-10-23) |
| Azure Quota REST | Automation is absent from the documented provider list. The service documents `Microsoft.Quota` registration and Quota Request Operator for quota operations. [Azure Quota API](https://learn.microsoft.com/en-us/rest/api/quota/) |
| Azure Support REST | The API manages support tickets. It provides no current regional quota reading. Listing tickets requires subscription Reader. Creating quota requests is a separate operation. [Azure Support API](https://learn.microsoft.com/en-us/rest/api/support/) |

Regional capacity therefore remains **unknown** in the app. Use the manual
view before deploying where Automation Account creation is expected. Published
default limits and account counts cannot establish a subscription's effective
limit. The existing Maester recovery view can record an operator-reported
rejection after an attempt. A quota reading does not reserve capacity.

## Contract for a future supported read

Keep a single host-owned read for Automation Account creation. Bind each result
to cloud, tenant, subscription, canonical region, resource type, reviewed
template identity and effective configuration, including whether the action
creates an account. Retain the approved evidence source and observed time.
Require a fresh, exact match at each previously supported deployment entry
point, including a new terminal handoff. Preserve the existing execution
approval and requirements gates. The provider and the host own endpoint and
permission selection.

Use these acceptance fixtures when a public provider contract is available.
For all cases, an already blocked action stays blocked.

| Deterministic input | Result and next action |
| --- | --- |
| Matching fresh read, usage 3 and limit 5, one account requested | **Available** for the existing authorized action. |
| Matching fresh read, usage 5 and limit 5, one account requested | **Exhausted**. Block the affected action and direct the operator to review the region or quota. |
| Count only, unknown limit, malformed values, or unsupported provider | **Unknown**. Offer the manual quota view. No capacity assertion. |
| Forbidden read | **Unknown**. Explain that the quota could not be read with the current access. Offer the manual view. |
| Selected region, tenant, account, source, configuration, or target changes, or evidence expires | Discard the previous result and any acknowledgment. An old reply cannot validate the new target. |
| Deadline or cancellation | **Unknown**. Ignore subsequent callbacks and offer explicit retry where the provider supports it. |
| Explicit retry returns a fresh exhausted result after an earlier timeout | Replace **Unknown** with **Exhausted** for the same current target. |
| Operator proceeds with unknown capacity under a future policy that permits it | Require a separate explicit acknowledgment bound to that exact target and check. Record the decision as unknown capacity. |

A provider adoption should demonstrate these cases with offline responses
before adding a runtime quota read. The documentation review and current
recovery tests exercise the separately reported failure path.
