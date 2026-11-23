# Conditional requirements

Manifest version `1.1` adds select-mode requirement conditions and Graph scope groups. Use version `1.0` for existing Boolean-only contracts. Older native clients reject `1.1`; putting new scope semantics into `1.0` is rejected by both the schema and native validator. Catalog document versions remain unchanged.

## Authoring

Declare an unconditional, non-sensitive controller with a typed default. Select controllers need unique declared string options. Requirements support equality only; a condition can reference a Boolean or, in `1.1`, a select field. Conditional fields/groups, secrets, acknowledgements and unknown controller values cannot control requirements.

```json
{
  "schemaVersion": "1.1",
  "prerequisites": [],
  "connections": [{
    "id": "graphPowerShell",
    "name": "Microsoft Graph PowerShell",
    "required": true,
    "reason": "Read the baseline policy and optionally apply changes.",
    "scopes": ["Directory.Read.All"],
    "scopeGroups": [{
      "id": "enforced-policy",
      "condition": { "field": "mode", "equals": "enforced" },
      "scopes": ["Policy.ReadWrite.ConditionalAccess"]
    }]
  }],
  "configuration": { "groups": [{
    "id": "mode",
    "title": "Policy mode",
    "fields": [{
      "id": "mode",
      "environmentVariable": "MODE",
      "label": "Mode",
      "type": "select",
      "default": "plan",
      "options": [
        { "label": "Plan", "value": "plan" },
        { "label": "Enforced", "value": "enforced" }
      ]
    }]
  }] }
}
```

Keep one row per registered connection provider in `1.1`. Only `graphPowerShell` supports `scopeGroups`. Its effective scopes are the base scopes plus active groups, sorted and deduplicated without case sensitivity. A maximum of 64 distinct effective Graph scopes is supported; inactive groups contribute no requested scope. Groups are bounded to 32 per connection, with unique IDs and 1–64 scopes per group. This metadata never executes author-provided commands.

## Native review and execution

The native snapshot contains the reviewed source contract digest, selected environment and Azure target, normalized controller values, effective tools (including the minimum azd version), connections/scopes and registered permission checks. Its separate snapshot version is `1.0`. The renderer displays this data and sends its digest with probes; it cannot authoritatively omit checks or replace scopes.

The wizard plan retains the snapshot. Preparation recomputes it from actual submitted values and compares it with the one-use approval. Typed defaults are persisted even when omitted, replacing older existing-project choices. Lifecycle approval resolves the saved contract and reads the target and declared controllers from one bounded environment-file payload. After approval consumption, native readiness checks use the effective set and reread it before launching. Both expansion and reduction reject an old approval. In-app environment writes clear outstanding approvals, including an A → B → A edit sequence.

Readiness checks inspect tools, required provider context and registered permissions using existing authentication. Requested scopes and retained UI hints are not provider proof. Missing fresh scope evidence fails closed; this feature does not add authentication, grant consent or change providers. Acknowledgement proofs remain a separate requirement. Removing resources with `down` retains its separate deletion approval without expansion acknowledgements; `refresh` remains outside execution approval.

Graph readiness uses the [owned Graph session](graph-session-boundary.md) for
its registered reads. A saved account hint cannot establish current grants.
Generic Graph template hooks, initialization, lifecycle and terminal handoffs
still require a reviewed execution adapter. The registered readiness result
applies to the read performed in the owned process.

The requirements snapshot was wired in [PR #172](https://github.com/nathanmcnulty/azd-gui/pull/172).
The same native snapshot supplies prerequisites, connections, scopes and
registered checks. The renderer preserves its digest, while preparation and
saved-project execution recompute and compare the effective requirements at
their native approval boundaries.

The wizard clears permission results on requirement-context edits and discards late replies. Successful native checks are discarded if a saved requirement context changes while probes run. Files remain mutable: reads cannot detect an unobserved external A → B → A sequence or prevent edits after launch.

## Audit and compatibility

Local database schema `5` stores the complete native effective snapshot alongside its digest in `requirement_snapshots`. Lifecycle entries use the operation ID; preparation entries record a unique execution intent. These records describe requested requirements, not successful deployment, consent or permission proof. Clearing local application data removes them. The external deployment receipt/evidence contract is unchanged.

Absent `scopeGroups` are omitted from legacy saved-contract serialization, preserving existing source digests. A frozen synthetic `1.0` snapshot is tested against its original digest and was also opened by the unchanged pre-feature Windows app. New shared fixtures exercise plan/enforced scopes, native caller recomputation, forged/omitted arrays, saved-context changes and approval consumption.

Reducing requested scopes does **not** revoke existing consent. Review consent separately using the provider's own controls.

## Provider-backed acknowledgements

An acknowledgement field can opt into complete verified tenant options using
`acknowledgementProvider`. The initial registered provider is
`entra-pim-role-definitions` with a `privileged`, `lessPrivileged` or `all`
classification. The optional `selectedIdsField` names an unconditional,
non-sensitive ID input. The provider view and exact selected IDs participate in
the proof alongside the verified tenant and existing controller values.

The app uses the complete role inventory obtained by the existing registered
PIM planner refresh. A successful refresh retains a native observation for
five minutes. A new refresh clears the prior observation and returns a new
revision only after a complete result. The digest covers the normalized role
records sorted by ID, including a replacement with the same count. Empty,
incomplete, expired or unclassifiable tier results cannot support an acknowledgement.
The button in the configuration screen rereads local evidence. The PIM planner
owns role refresh and its existing verified-session requirements.

The [offline azd-pim example](../src/lib/fixtures/pim-provider-acknowledgement.json)
adds the declaration to the all-privileged-role acknowledgement. Its
less-privileged acknowledgement remains independent. Native approval and launch
check current provider evidence. Provider proof stays in memory and does not
change exported azd variables or the existing CLI acknowledgement guard.
Existing manifests keep their current behavior. The bundled selected-role PIM
workflow keeps its existing review and execution path.
