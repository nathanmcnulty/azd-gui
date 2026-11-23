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
native revision that advances on GUI environment writes and source refresh.
A failed verification or confirmation discards the pending native review.
After a mismatch or failed request the panel reloads the target for the
operator to inspect before another explicit verification. If the source
changed, reopen the project to load its current reviewed contract.

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
