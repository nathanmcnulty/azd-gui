# Application registration ownership evidence

This provisional internal contract is the first local slice of #59 and the
input to #161. It defines one host-owned verifier for an existing Entra
application registration. The ownership path must keep the template's CLI
validation authoritative. The local review path uses the contract with an offline transport boundary.

## Verification

The registered provider declaration names a field containing the exact
application **object ID**. The optional application ID field identifies the
client ID the operator expects to find. Both fields must resolve from the
reviewed source to non-sensitive stable UUID values. The provider owns the
single application read and its credentials. Display names are presentation
only. The contract carries field names and has no endpoint or executable input.

The first fixture models azd-pim's revocation application
`AZD_PIM_REVOCATION_APPLICATION_OBJECT_ID` and
`AZD_PIM_REVOCATION_APPLICATION_CLIENT_ID`. The expected verified properties
for this slice are one exact object ID and application ID, single-tenant
audience, the template's derived identifier URI matching, and access-token
version 2. A legacy record with only an object ID can be matched by that exact
ID and its safe app ID then projected from the verified result. A client-ID-only
fallback is outside the first GUI verifier. The CLI retains its current guards.

Bind a result to the verified public cloud, tenant and operator, the exact
subscription, intended target resource and environment, reviewed source and
normalized current inputs. The native host supplies those bindings. Use
`foreign` for a different verified tenant or operator, `mismatch` for changed
object or expected properties, and `unknown` for cancelled, denied,
unavailable or incomplete reads. Unverified results carry a fixed reason and
omit observed foreign identifiers, raw Graph data and proof fields. The safe
verified projection carries UUIDs, Boolean comparisons and token version.
It never contains the actual identifier or callback URI.

## Offline application lookup

The native verifier constructs one Graph v1.0 GET for the saved application
object ID. It selects id, appId, signInAudience, identifierUris, and api.
The owned transport must attest the authorized tenant and operator before
and after the read. The single-object response must identify the application
resource type. The projection rejects collection responses, service
principals, agent identity blueprints, and unsupported shapes.
See [Microsoft Graph Get application](https://learn.microsoft.com/en-us/graph/api/application-get?view=graph-rest-1.0).

The pinned azd-pim producer derives the expected URI as
api://{workflowHost}/{applicationId}. The native verification needs a
workflow DNS host independently bound to the saved deployment target.
An unavailable host keeps verification unavailable, including when the
Graph body contains a plausible URI. Confirmation checks the same host
again before consuming the pending verification. The mock provides its
reviewed host independently of the mock application response.
See [the pinned azd-pim producer](https://github.com/nathanmcnulty/azd-pim/blob/97054a7c8bb67c4a4de52caccb68442c1e1ce769/scripts/AzdPim.Optional.psm1#L562-L587).

An explicit null requestedAccessTokenVersion has effective version 1,
which mismatches this provider's required version 2. An omitted version
or api projection leaves verification incomplete.
See [the Microsoft Graph apiApplication contract](https://learn.microsoft.com/en-us/graph/api/resources/apiapplication?view=graph-rest-1.0).

The owned Windows worker implements this read as a fixed application action.
It requires a completed session probe, the same tenant and operator, an
existing credential for that identity, and Application.Read.All in both the
configured and observed delegated scopes. It sends one GET with a ten-second
deadline and accepts at most 8 KiB of a successful response. The native verifier
consumes that response and exposes only the documented safe evidence.

Production verification remains unavailable. The existing MSAL worker offers
no eligible application credential without acquisition or refresh. The saved
project also lacks an independently verified workflow hostname. A synthetic
MSAL/HTTP fixture exercises the Windows worker and the explicit local review
while those prerequisites remain open.

## Workflow hostname producer contract

The pinned azd-pim ARM deployment outputs
`AZD_PIM_REVOCATION_LOGIC_APP_RESOURCE_ID`. Its post-provision hook obtains
the callback for that exact workflow and uses its hostname to build the
application ID URI. The deployment report, applied report, state, and AZD
environment values are mutable project files. Even a generic deployment
receipt with a `current` correlation state establishes context only. The GUI
operation history does not capture any of these output bytes.

The provisional [observation schema](../schemas/internal/azd-pim-workflow-host-observation.schema.json)
specifies the two values the producer would emit. Take the lowercase ARM
resource ID from the exact workflow output and the hostname from the ARM
callback the hook already obtained for that resource. Keep the candidate pair
in memory until the optional session-revocation stage, role linking, and the
complete deployment finish successfully. The current hook can write a partial
report after observing the host and then fail. A partial report supplies no
evidence, even when it contains matching fields. Emit the pair only on
success. Reuse the existing callback observation without another ARM
request or a substitution from `accessEndpoint`. The hostname uses lowercase ASCII DNS
labels of 1–63 characters and a final label of 2–63 ASCII letters, with at
most 253 characters in total. Keep the callback URL, path, port, query,
signature and Graph ID URI out of the payload. The
[fixture](../src-tauri/tests/fixtures/azd-pim-workflow-host-observation.json)
uses synthetic values in the shape of the Bicep output and callback hostname.

The native operation runner has a separate opt-in for a registered producer.
It checks the reviewed revision and the digest of the producer bytes it
executes. The native caller must supply the independently resolved workflow ID
for that exact deployment. The child receives a private output pipe reserved
for one UTF-8 JSON observation of at most 1 KiB. Ordinary azd stdout, deployment
reports and environment output files never enter that pipe. The capture checks
the full resource ID, public cloud, tenant, subscription, resource group,
registered project, environment generation and reviewed source contract.
It issues a process-local receipt only after the matching operation succeeds
and that terminal result is persisted. The capture permits writes to the pinned
producer's known deployment outputs. Every other saved value must match the
admitted inputs. The application object and client IDs are also legacy selectors.
Each nonempty admitted ID must remain the same. A missing ID may be filled by a
successful producer, which must write a complete pair of valid IDs. The final
saved workflow ID must match both the independent
native ID and the child observation. The saved file corroborates that match
without supplying hostname provenance. Failure, cancellation or incomplete
output leaves the capture unavailable.

The receipt binds the exact final environment snapshot and the native project's
monotonic selection revision. The revision advances on saved environment or
source changes and explicit GUI target edits. The PIM picker shares the native
operation guard with generic selection. Returning to an earlier environment or
source cannot restore a prior receipt. A later operation that could have
affected resources, including a pending or failed operation, supersedes it.
Graph application properties supply no hostname evidence.

The native registry pins azd-pim revision
`5d22767595740d5e3d1aafccecf3435bf0e9b9fc` at the repository root with a saved
application ownership opt-in. The lane requires enforced session revocation
with notifications disabled. It records the raw hashes of the hook declaration,
entry point, post-provision modules, and vendored Graph authentication manifest
and module. The pin covers the revocation-only source lane. The polling package,
externally resolved Microsoft.Graph.Authentication module (minimum 2.30), Azure
CLI, and azd are outside this qualification.

The Windows offline qualification verifies all pinned bytes before running
the actual entry point with a synthetic nested hook. It uses the same executable
resolver as the app, checks the selected executable against the independently
measured local file, and launches with the native operation metadata, a private
working directory, and isolated PowerShell module search. It exercises
successful and partial outcomes, source drift, and independent workflow ID
mismatch. It runs no production hook, Azure CLI, or Graph module.

Production verification stays unavailable. The ordinary azd lifecycle still
launches its own post-provision hook, which can authenticate interactively and
uses ambient CLI and module resolution. The remaining adapter is the
native-owned deployment handoff that executes the reviewed stage once under
the admitted target and supplies the independently observed workflow ID from
that same deployment. A second hook run or a saved output file cannot supply
that handoff. The internal schema remains provisional.

## Ownership receipt

A verified result describes the exact object at a point in time. It does not
establish ownership by itself. Migration requires matching legacy stable IDs
and an explicit reviewed migration. Adoption requires a separate exact
acknowledgement of the current verified result. The host must record the choice
only after the supported action confirms its outcome. A `created` receipt
requires evidence from registered creation. A GET or a matching display name
cannot create a receipt. A missing or mismatched binding blocks adoption.

The immutable receipt carries its original binding, app ID, verification time,
record time, ownership source and digests. The digests detect changes. An
exported receipt carries historical evidence and does not establish trusted
local ownership on import. The native operation that recognizes durable state
must compare the original binding and its own operation provenance. Edits or
rebinding require a new verification and, where applicable, a new explicit
decision. Keep old receipts as historical records.

Use SHA-256 over UTF-8 compact JSON arrays. Lowercase UUIDs and hexadecimal
digests before serialization. The fixed binding array is `[provider, cloud,
tenantId, subscriptionId, environment, operatorDigest, targetResourceDigest,
sourceContractDigest, inputsDigest, objectId, expectedApplicationIdOrNull]`.
Verification uses `["application-registration-verification", bindingArray,
checkedAt, [objectId, applicationId, singleTenant, identifierUriMatches,
accessTokenVersion]]`. A receipt uses `["application-registration-ownership-receipt",
bindingArray, applicationId, ownershipSource, verifiedAt, recordedAt,
verificationDigest]`. Preserve the authoritative timestamps exactly.

The [internal schema](../schemas/internal/application-registration-ownership.schema.json)
and [synthetic fixture](../src/lib/fixtures/application-registration-ownership.json)
exercise this contract offline.

## Local review

A saved project can opt in with a reviewed applicationOwnership declaration.
The declared fields stay ordinary typed configuration inputs. The project
Overview and the registered PIM workspace share the review panel. A reviewed
PIM contract must opt in. The bundled pilot's existing contract and behavior
stay unchanged.

The native context command reads the exact registered project and the same
bounded environment file for the public Azure tenant, subscription, resource
group and stable application IDs. It returns those typed values, the
reviewed source and target digests, and an opaque context digest. The general
environment view keeps its existing redaction rules. Neither context nor
review returns raw environment content. The target resource digest binds the
saved resource-group scope.

Verify and Confirm reread the exact native target and require the displayed
context digest. It also binds the environment's modification time and a
native revision that advances on GUI environment writes, including selecting
a saved PIM environment, and on source refresh. Failed verification,
confirmation, or context retrieval discards the pending native review.
A context read that observes a changed target also discards the review.
When an Overview refresh finishes for the displayed project and environment,
the ownership panel clears its evidence and reloads that saved target.
The operator can inspect the current target before another verification.
If the source changed, reopen the project to load its current contract.

The verifier has an unavailable production transport in this slice. Synthetic
tests exercise the native registered caller and the project review view.
An unverified IPC response is only a status and fixed reason, since an
unavailable or foreign identity cannot supply the verified evidence binding.
The verified response uses the internal evidence format.
A verified observation can enable the explicit review decision. Context edits
clear the view. The native decision consumes the prior verification once
and checks the current saved context and verified operator. The review receipt
is session evidence. It has its own `ownershipReviewDecision` kind and carries
no ownership source. It does not persist or authorize an adoption. Native
receipt provenance and a separately qualified provider and adoption action
remain follow-up work.

The review receipt hash uses `["application-registration-review-receipt",
bindingArray, applicationId, verifiedAt, reviewedAt, verificationDigest]`,
with the same compact UTF-8 JSON encoding as the verification.
