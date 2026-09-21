# Management evidence correlation contract

Status: implemented by the azd-reference producer at merge
`08e60886a8efc2c274f7ed9e2c2527e1771c1603` and the azd-gui reader. A template
must still adopt the producer inputs before it emits correlated evidence.

Deployment receipts and validation reports are local, producer-written records.
They can help an administrator understand a deployment only when the app can
compare their claimed context with independent state it already holds. They do
not authenticate the producer or prove Azure state, message delivery, or
cleanup.

## Compatible extension

Unbound legacy output remains at outer `schemaVersion: "1.0"`. Bound output uses
outer `schemaVersion: "1.1"` and puts one shared object at
`details.azdManagementEvidence` in a deployment receipt or at
`environment.metadata.azdManagementEvidence` in a validation report. The
namespace uses existing extension maps, so the only outer-envelope change is
the explicit version. Existing readers reject 1.1 output instead of silently
showing a producer-reported pass while ignoring a known mismatch. Updated
readers continue to accept 1.0 as `legacyUnbound`. New producers should use
typed parameters rather than asking templates to construct this object
manually; supplying a binding selects 1.1, while omitting it retains 1.0.

```json
{
  "version": "1",
  "evidenceClass": "validation",
  "binding": {
    "project": {
      "id": "opaque-project-id",
      "environment": "dev"
    },
    "target": {
      "azureCloud": "AzureCloud",
      "tenantId": "00000000-0000-0000-0000-000000000000",
      "subscriptionId": "00000000-0000-0000-0000-000000000000",
      "resourceGroup": "rg-example-dev"
    },
    "source": {
      "templateId": "example",
      "revision": "0123456789abcdef0123456789abcdef01234567",
      "contractDigest": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
    },
    "operation": {
      "id": "00000000-0000-0000-0000-000000000000",
      "kind": "validate"
    }
  },
  "nextActions": [
    {
      "code": "reviewWarnings",
      "owner": "operator",
      "priority": "required"
    }
  ]
}
```

The namespace is absent for a legacy or deliberately unbound artifact. If it is
present, every object and required field must be complete. A partially populated
namespace is invalid rather than weak evidence.

`contractDigest` is optional because the reference component is not specific to
this GUI. When present, it is the digest of the locally retained reviewed
contract. It is not publisher authentication or a digest of the current working
tree. `resourceGroup` is optional because not every target is scoped to one
resource group. An omitted optional value is `unverifiable` whenever the GUI
has a corresponding expected value. The literal revision `local` is always
`unverifiable` because the GUI does not hash the current working tree. Either
case yields `insufficientContext`, never `current`. Every identifier remains
inside the native reader and is reduced to comparison states before crossing
into the renderer.

## Bounded values

`evidenceClass` separates claims that must never upgrade one another:

| Value | Meaning |
| --- | --- |
| `validation` | Checks ran in plan or verify mode. |
| `resourceMutation` | The producer recorded intended or applied resource or directory changes. |
| `delivery` | A delivery-mode validation attempted or observed a delivery-specific check. |
| `cleanup` | The producer recorded a cleanup operation. |

A validation report normally uses `validation`; `mode: delivery` requires
`delivery`. A receipt uses `resourceMutation` or `cleanup`. A passed validation
does not establish successful mutation, delivery, or cleanup. A cleanup receipt
does not establish that independent cleanup verification passed.

Operation kinds are `provision`, `deploy`, `up`, `down`, `validate`, `delivery`,
and `cleanup`. IDs are UUIDs. Revisions are a 40-character Git SHA or the literal
`local`; digests are 64 lowercase hexadecimal characters. IDs, environment,
template, cloud, and resource-group strings are bounded and reject controls.
Tenant and subscription IDs are UUIDs.

`nextActions` is limited to 20 entries. Producers may select only these codes:

- `reviewWarnings`
- `retryOperation`
- `verifyResources`
- `verifyPermissions`
- `proveDelivery`
- `completeCleanup`

Owners are `operator`, `subscriptionOwner`, `identityAdmin`, `messagingAdmin`,
`applicationOwner`, or `templateMaintainer`; priority is `required` or
`recommended`. The renderer maps each code and owner to app-owned text. It never
renders producer strings, HTML, URLs, diagnostics, `details`, evidence maps,
expected/actual values, or artifact paths. Existing free-form
`operationalActions` remain in the receipt for compatibility but are counted
only and never displayed.

## Reader result

The native reader returns two independent axes:

| Axis | Values |
| --- | --- |
| File state | `available`, `notFound`, `invalid`, `futureVersion`, `unsafePath`, `tooLarge` |
| Binding state | `current`, `stale`, `mismatch`, `legacyUnbound`, `insufficientContext` |

An available bound artifact also has internal project, environment, target,
source, and operation comparisons of `match`, `mismatch`, or `unverifiable`.
Only the aggregate enum crosses into the renderer.

Binding precedence is deterministic:

1. Any exact mismatch produces `mismatch`.
2. A supported 1.0 artifact with no namespace produces `legacyUnbound`.
3. A missing independent comparator produces `insufficientContext`.
4. When every dimension matches but a later persisted resource-mutating
   operation exists, the artifact is `stale`.
5. Otherwise the artifact is `current`.

Staleness begins with an exact matched operation ID. Timestamps may order later
persisted operations only after that identity match; age alone never establishes
identity or freshness. A later failed or cancelled mutation also makes the older
artifact stale because it may have changed resources partially.

Only `current` evidence may contribute a positive UI status, and a
producer-reported failure always remains failed. Plans, legacy output, stale
output, and insufficient context remain neutral or warning. Mismatch, malformed,
and future-version output cannot make a project appear healthy.

The producer asserts outcomes, checks, counts, evidence class, binding values,
and next actions. The GUI independently verifies only the fixed report path,
size, supported shape and semantic consistency, exact comparisons with its
registered project, selected azd environment, saved source contract and
persisted operation context, and whether a later mutation superseded the
artifact. It does not independently verify cloud resources, identity changes,
delivery, cleanup, authorship, or current working-tree content.

## Implementation sequence

1. In `azd-reference`, add schema 1.1 bound-output support while retaining 1.0
   legacy output, and define the shared namespace in both schemas while
   preserving other extension-map content. Add typed binding/action parameters
   to both PowerShell producers, update standards and fixtures, reject unknown
   fields and values, and bump both component versions.
2. Merge and tag that reference change, then record its exact revision and
   component versions in the GUI compatibility fixtures.
3. In the GUI, parse the optional namespace in both native readers. Build the
   independent comparison context from the registered project, saved template
   snapshot, selected azd environment values, and persisted operation history.
4. Add one nullable `context_json` field to persisted operation history. Store
   the approved environment, target, source revision, and reviewed contract
   digest for GUI-started operations. Legacy rows remain readable and yield
   `insufficientContext` when they are needed for comparison.
5. Project only file/binding enums, existing safe counts, and allowlisted next
   actions to the renderer. Show outcome and correlation as separate facts.
6. Adopt the producer in an actual template separately. An external-terminal
   artifact cannot be `current` until the consumer deliberately propagates a
   GUI-issued operation context and the GUI persists that handoff.

Producer tests must generate bound receipt and validation fixtures that
round-trip through both GUI readers. Coverage includes legacy/unbound output,
each mismatch dimension, missing context, exact operation match, later-operation
staleness, malformed partial binding, future outer and namespace versions,
contradictory totals, plan/enforce and evidence-class rules, action allowlists
and limits, secret/raw-text stripping, and existing unsafe-path and size cases.
No reader follows an artifact path from a receipt.
