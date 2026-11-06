# Conditional acknowledgements

The first slice of [#56](https://github.com/nathanmcnulty/azd-gui/issues/56) adds a
declarative `acknowledgement` field to the deployment wizard. Use it when an
optional configuration choice needs a separate, explicit acknowledgement of its
consequences. It is a required checkbox while active, with no initial acceptance.
It is not an executable command, a permission grant, or proof that the selected
account can perform a deployment.

## Authoring the contract

The manifest uses the existing `schemaVersion: "1.0"`. Add an unconditional,
non-sensitive controller and an acknowledgement with its own condition:

```json
{
  "id": "directoryOptions",
  "title": "Directory options",
  "fields": [
    {
      "id": "includeDirectoryChanges",
      "environmentVariable": "INCLUDE_DIRECTORY_CHANGES",
      "label": "Include tenant-wide directory changes",
      "type": "boolean",
      "default": false
    },
    {
      "id": "acknowledgeDirectoryChanges",
      "environmentVariable": "ACKNOWLEDGE_DIRECTORY_CHANGES",
      "label": "I understand that this option changes tenant-wide directory settings.",
      "help": "Review the selected tenant and the enabled option before continuing.",
      "type": "acknowledgement",
      "required": true,
      "condition": { "field": "includeDirectoryChanges", "equals": true },
      "validationMessage": "Acknowledge these directory changes to continue."
    }
  ]
}
```

An acknowledgement must:

- Declare `required: true` and a field-level `condition`. A conditional group does
  not replace the field condition. A group condition and field condition must
  both match for the acknowledgement to be active.
- Use a nonempty `label` for the checkbox's accessible message. Optional `help`
  explains the consequence; keep it specific enough to make an informed choice.
- Omit `default` entirely, including `false`. Defaults cannot record consent.
- Omit `lookup`, `options`, `pattern`, and `placeholder`, even if empty.
- Omit `sensitive` or set it to `false`. Acknowledgements and their controlling
  values must not carry secret data.
- Optionally declare a nonempty string `validationMessage` for a missing
  acknowledgement. This property is not supported on other field types.

Both the field condition and any containing group condition must reference
declared, non-sensitive controller fields outside conditional fields or groups.
An acknowledgement cannot control another acknowledgement. Existing condition
operators remain available: `equals`, `notEquals`, or `includes`, with exactly
one operator per condition. Comparisons must match the controller's type;
`includes` is for a declared multiselect value. Multiselect acknowledgement
controllers cannot use empty option values or commas, so their saved values
round-trip without losing a selection. This first slice does not add
arbitrary expressions, chained conditional controllers, or executable lookups.

The [JSON schema](../schemas/azd-gui.schema.json) enforces the local shape and
forbidden properties. Native manifest validation additionally resolves field
references, checks controller types and unconditional placement, and rejects
recommendation profiles that try to assign acknowledgement values. Schema
validation alone is not the complete trust check.

## Fresh acknowledgement and review

Only the administrator checking the active checkbox creates acceptance. Applying
a recommendation profile, loading a saved draft, restoring a previous deployment,
or supplying an environment variable with `true` cannot create a fresh
acknowledgement. Recommendation profiles may select the feature that activates
the checkbox, but cannot select the checkbox itself.

Acceptance is scoped to the selected tenant and the typed current values of all
controllers used by the field and its containing group. Changing that context
requires acknowledgement again. Hidden acknowledgements cannot retain effective
acceptance or contribute an affirmative environment value. Drafts do not persist
accepted checkbox values or acknowledgement evidence.

The review presents each active acknowledgement beside:

- Its message and acknowledgement state.
- The field and group conditions and their current controlling values.
- The selected tenant.
- The declared environment variable name that will carry the acknowledgement.

The selected environment name remains part of the overall deployment review.
The administrator can therefore review the consequence, configuration, tenant,
and persisted variable together before preparation.

## Native execution boundary

Preparation submits an acknowledgement proof separately from the environment
values:

```json
{
  "fieldId": "acknowledgeDirectoryChanges",
  "tenantId": "11111111-1111-4111-8111-111111111111",
  "controllingValues": {
    "includeDirectoryChanges": true
  }
}
```

`controllingValues` keys are configuration **field IDs**, not environment variable
names. Values retain their JSON types: booleans, strings, or arrays of strings.
If a group condition uses a different controller, that controller is also present
in the mapping. If both conditions use the same controller, it appears once.
The frontend captures this context only on checkbox interaction and does not
reconstruct accepted proofs from profiles, drafts, or stored environment values.

Native preparation independently evaluates active group and field conditions,
checks the acknowledgement's literal boolean value, and compares the submitted
proof with the tenant and controller values being prepared. Preparation must
explicitly supply every acknowledgement controller, including controllers whose
current choice makes the acknowledgement inactive; null or omitted controllers
cannot retain an older existing-project choice. Inactive acknowledgement values
are persisted as `false`. Optional controllers without defaults start with the
displayed empty string, `false`, or empty selection, and those explicit values
are included during preparation. Missing or stale evidence blocks preparation rather than silently accepting a truthy string or
previously saved value.

The same checks apply to `preview`, `up`, `provision`, and `deploy` for saved
projects, using the selected environment's saved values. Declared controller
defaults remain available when a value has not been saved. Without a default,
a missing saved controller blocks the operation until the administrator
explicitly saves the displayed configuration; opening or inspecting a project
never initializes values by itself. Optional select controllers may be blank;
required selects and unknown options are rejected. `down` and `refresh`
are not gated by this expansion acknowledgement. A fresh proof may be reused
across preview and deployment while the same session and context remain open;
cancellation, failed approval, context edits, and closing/reopening require a
new checkbox interaction. Switching or refreshing a project environment also
clears the session's proof conservatively.

For a new project, `azd init` runs before the configuration variables are
applied. The gate validates the reviewed acknowledgement before initialization;
it does not inject those later environment variables into init hooks. Templates
must keep initialization safe and enforce their own consequential operations.
Existing-project preparation skips `azd init`.

This evidence is context validation across the GUI/native boundary, not a signed
attestation of human consent. It does not protect against a caller that can forge
the complete request, a hostile process running as the same user, or changes to
a mutable project after preparation. It does not verify provider-side content,
membership, affected-object counts, or effective write permissions.

## Compatibility and deferred work

The schema version remains `1.0`: older native clients reject the unknown
`acknowledgement` field type instead of treating it as an ordinary boolean. This
is not a guarantee that all other manifest consumers enforce acknowledgements.
Authors must continue enforcing consequential choices in template-side scripts
and hooks for direct CLI use, older clients, and existing-project lifecycle
operations. An already persisted acknowledgement environment variable is not
durable proof of fresh consent to a later operation.

This slice supplies the generic manifest, wizard, review, and native execution
contract. Provider-bound membership digests and affected-object counts belong to
[#160](https://github.com/nathanmcnulty/azd-gui/issues/160), after the discovery work
in [#41](https://github.com/nathanmcnulty/azd-gui/issues/41). Template adoption in
[#59](https://github.com/nathanmcnulty/azd-gui/issues/59) and
[#161](https://github.com/nathanmcnulty/azd-gui/issues/161) is deferred. This change
does not declare those provider or template integrations complete and does not
expand Azure privileges, run deployments, or change release qualification.

Run local validation under the [Actions usage policy](actions-policy.md).
The focused schema regression suite is `npm test -- src/lib/schema.test.ts`;
full repository verification remains `npm run verify`. Passing schema fixtures
alone does not establish native rejection behavior or accessible UI behavior.
