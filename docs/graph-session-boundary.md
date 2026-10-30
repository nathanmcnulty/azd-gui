# Owned Graph session boundary

Graph permission evidence applies only to the registered read performed in the
owned PowerShell worker. It cannot authorize independent `azd` hooks, initialization,
lifecycle commands (including removal), or external terminals. Templates declaring
a Graph connection or delegated-scope check are rejected at those execution routes
until their protected operations adopt a reviewed execution adapter. Successful
readiness checks do not establish write authority.

## Credential and evidence ownership

Rust resolves the effective requirements from a native reviewed contract or saved
project. The binding includes its digest, explicit expected account, tenant,
application client ID, public Graph resource, requested delegated scopes and a
registered read URI. A native dialog presents this exact binding before sign-in.
There is no default client or inference from another tool's signed-in account.

One private `pwsh -NoProfile -NonInteractive` process hosts an embedded managed
adapter and its own MSAL public-client application. Sign-in uses the system browser
and `http://localhost` redirect. The application must already support that public
client flow. No broker, device-code fallback, shared cache registration or cache
serialization is installed. The adapter does not call `Connect-MgGraph`,
`Get-MgContext` or `Invoke-MgGraphRequest`, or read/write `mg.authrecord.json`.

Initial authentication validates the actual MSAL result's account, home-account
identity, tenant-local user identity, tenant, public authority, token type, expiry,
client configuration and actual delegated grant set. Every probe forces a silent
refresh for the retained account object and exact resource-qualified scopes.
Interaction-required errors fail the session. Changed identity or grants invalidate
it, including additions and reductions that still cover the required subset.
Initial actual grants may exceed requested grants; the interface displays the
actual returned set. Reducing requirements or disposing a session does not revoke
consent already held by the account/application.

The same returned access token is used only inside managed code for that read's
Authorization header. HTTP redirects are disabled and no application retry is
installed. Only whitelisted account/tenant/client/resource/grant metadata crosses
the worker protocol. Tokens, authentication results, response bodies and raw SDK
errors are excluded from PowerShell output, Rust, the renderer and persistence.

The fixed read registry is:

| URI (public Graph v1.0) | A required permission that covers this read |
| --- | --- |
| `/organization?$select=id` | `Directory.Read.All` or `Directory.ReadWrite.All` |
| `/identity/conditionalAccess/policies?$top=1&$select=id` | `Policy.Read.All` or `Policy.ReadWrite.ConditionalAccess` |
| `/me?$select=id` | `User.Read` |

Only an already requested permission selects a probe; no extra permission is added
to manufacture evidence. Unknown-only scope sets and `.default` are rejected.
There is no renderer-selected URL, script or protected-write dispatch.

## Lifecycle

A native generation ticket is reserved before approval or process startup.
Disposal, replacement, configuration/identity changes and a changed effective
requirement digest revoke both pending and installed sessions. Execution approvals
are cleared on these changes or failed evidence checks. Renderer request epochs
discard late sign-in and connection refresh completions.

Each response must match the worker PID, session nonce, request sequence, action
and requirement digest. Responses are bounded. Session lifetime is 15 minutes,
read timeout 30 seconds and initial sign-in timeout 180 seconds. Failure,
cancellation, timeout, EOF or a dropped native caller closes the process and its
process tree. A failed session cannot be reused. Disposal frees the in-memory
credential by ending its owning process; it does not sign out external tools or
change tenant consent.

## Build and offline validation

Native builds, lint and tests require .NET SDK **8.0.4xx**. The adapter's
`global.json` selects the latest installed patch in that feature band. Cargo
publishes the managed project with locked restore from the public NuGet feed and
embeds its DLLs; the test assembly is excluded. MSAL is pinned in the project and
package lock. The validation and release workflows explicitly provision this SDK.
End users need PowerShell 7.4 or newer, which supplies the managed runtime; the
.NET SDK is a build prerequisite only.

After loading the normal native compiler environment, run:

```powershell
./tools/Test-GraphProcessAdapter.ps1 -BuildRoot <local-build-directory>
npm run verify
```

The adapter script builds a separate test-only MSAL/network substitute and runs
the actual Rust → PowerShell → managed validation and HTTP dispatch path. It never
authenticates or accesses Graph. Cases include separate processes, absent/expired
credentials, mismatched identity/client/resource/grants, scope expansion/reduction,
silent-refresh failure, denied/redirected reads, malformed/foreign/oversized replies,
cancellation during authentication/read, timeout, process death and late replies.
Production DLL initialization is also checked without authentication. App tests
cover cancellation, identity editing, close and stale connection refreshes.

Live browser sign-in and Graph reads require a separate exact approval identifying
the account, tenant, existing public client, scopes and registered read. Stop on
unexpected consent, permissions, account selection or alternate flow. Offline
passes do not qualify live authentication or generic template execution.
