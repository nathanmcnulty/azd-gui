# Account choice in the deployment wizard

Decision, 26 September 2026: add **target-aware connection choice**, not a
shared token manager. The admin chooses the Azure tenant and subscription first.
The wizard then shows the effective account for each required provider, whether
that provider was freshly verified for the selected target, and one clear action
when it was not. An optional "Locally visible profiles" expander can help an
admin recognize another account already known to Azure CLI or Az PowerShell.
"Other or new account…" starts that provider's normal native browser/WAM sign-in.

Each CLI owns its credentials and context. A profile listed on disk is a
**candidate**, not evidence of a usable token, consent, permission, or access to
the selected subscription. Do not build a universal account picker, read token
caches, display token values, or silently switch global CLI defaults. In
particular, do not call `az account set`, `Set-AzContext`, or logout to make a
wizard choice appear active. Never use device-code authentication.

## What the wizard should say

For each provider required by the selected template, show:

| Field | Meaning |
| --- | --- |
| Current account | The account reported by the provider's current-context probe; "Unknown" when unavailable. |
| Target check | Verified for the selected tenant/subscription, wrong target, expired, or unverified. A cached profile alone can only be unverified. |
| Next action | "Continue", "Reconnect this provider", or "Choose another account". State which provider needs attention. |

Show advanced IDs only where useful for target review. After any account or
target change, discard prior connection-readiness and Graph-grant evidence and
reprobe. Persist the selected environment and Azure target, but never persist a
selected account as authorization. On reopen, report the current provider state
and verify the target again before allowing preparation or lifecycle commands.

## Provider boundaries

| Provider | Current evidence | Optional locally visible choices | Selection behavior |
| --- | --- | --- | --- |
| Azure CLI | Existing target-aware `az account show --subscription <id>` probe | Bounded metadata from `az account list`; label as candidates | Explicit reconnect or provider-supported per-command target; do not alter the CLI default. Reprobe the exact target. |
| Az PowerShell | `Get-AzContext`, plus target-scoped `Get-AzContext -ListAvailable` | Bounded context metadata from `Get-AzContext -ListAvailable`; label as candidates | Native reconnect, then fresh exact-target check. Do not use `Set-AzContext` merely to satisfy the wizard. |
| azd | `azd auth status` plus fresh tenant check | No supported cross-account inventory in the current integration | Show current account and "Other or new account…"; rerun the target check after sign-in. |
| Microsoft Graph PowerShell | `Get-MgContext` or the existing single auth-record fallback | No general cache inventory | Show current context and native reconnect. An auth record with no scopes cannot establish grants. |

The candidate inventory should return bounded, allowlisted account label,
tenant, subscription, provider, and display name only. It must not return raw
CLI JSON or PowerShell context objects to the renderer or logs. Suppress empty,
duplicate, and malformed entries; keep an "Unknown" state when a probe fails.
If a provider cannot safely select a specific candidate without changing its
global default, present the candidate as a hint and offer reconnect rather than
a clickable account switch.

## Implementation packets

1. **Current-status UX.** Extend the Connections and wizard review to show the
   effective account and fresh target match for each required provider. Reuse
   `src-tauri/src/connections.rs`, `src/lib/connections.ts`, the connection
   controller, and existing bridge/types. Invalidate readiness on target,
   account, or tenant change. Test two cached identities where the wrong one is
   current, an expired current context, and no current account.
2. **Candidate inventory.** Add read-only, bounded Azure CLI and Az PowerShell
   metadata enumeration behind the optional expander. Do not infer token validity
   from enumeration. Cover duplicate tenants, duplicate accounts, malformed
   output, no installed tool, and a candidate that fails the exact-target probe.
3. **Explicit account transition.** Offer "Other or new account…" per provider
   through its normal browser/WAM flow; use one-use UI intent and a fresh native
   target check before enabling Continue. A real Windows 11 multi-account test
   should prove that selecting another account does not silently mutate Azure
   CLI or Az PowerShell defaults and that stale Graph grants cannot carry over.

These packets follow the current Windows 11 signed-candidate acceptance. They
should be separate, reviewable milestones; the clean-profile Nora sign-in and
target-rejection gates remain the immediate release work. A future CLI may add
a supported account-selection contract, at which point this design can be
revisited against that contract instead of parsing private caches.
