# Guided template metadata

Keep infrastructure, hooks, and optional-component behavior in the template. An `azd-gui.json` describes inputs and required tools; it does not supply executable commands or authorize deployment. The schema is [azd-gui.schema.json](../schemas/azd-gui.schema.json).

## Optional feature requirements

Prerequisites, connections, and registered permission checks can declare a boolean condition:

```json
"condition": { "field": "includeExchange", "equals": true }
```

The controller must be a non-sensitive boolean configuration field with a boolean default, outside conditional fields/groups. Only boolean equality is supported for requirements. Feature groups containing these controllers appear before tools and connections so administrators can make the choices that determine readiness. Other configuration groups retain their normal placement after environment selection.

The GUI recalculates tools, connections, and read-only checks after edits or profile application, invalidates review/preview, and ignores stale check results. Native preparation uses the submitted environment values; existing-project lifecycle commands re-read the selected environment before checking required tools. Values must be literal `true` or `false`; absent values use the declared default. This does not freeze a mutable project environment against external edits between checking and execution.

Keep requirements needed by baseline setup unconditional. The Maester contract gates its host `ExchangeOnlineManagement` module on Exchange coverage and a dashboard-specific Graph reachability check on the dashboard option. ARM and Graph baseline access remain required even for the Minimal pilot profile. Read-only checks do not prove write, role-assignment, or consent authority. Templates must implement the same feature choices themselves.

## Security group input

Set `"lookup": "securityGroup"` on a non-sensitive `entraObject` field to offer a name-prefix search alongside manual GUID entry. Maester's optional dashboard uses this for its allowed security group. Unknown lookup names and lookups on other field types are rejected.

The deployment wizard searches only after an explicit administrator action and after a tenant/subscription is selected. It verifies the cached Azure CLI account and Graph organization against that target, queries public-cloud Microsoft Graph for security-enabled groups, and shows at most 25 names and IDs. Names are not unique: the administrator must explicitly select a result. Refine the prefix when more matches exist. Changing the Azure target clears these fields, including manually entered IDs; changing a search discards stale results.

Search uses existing Azure CLI authentication and directory read access. It never signs in, grants permissions, changes membership, or proves deployment authority. Failures preserve manual ID entry. The existing-project configuration editor currently offers manual entry only. Search results are not saved; the selected ID becomes ordinary template configuration.

## Recommendation profiles

Profiles are optional starting points that an administrator explicitly applies. Keep existing template defaults unchanged. Explain permission, resource, and operational consequences in each profile description; do not claim that a smaller deployment needs no administrative permissions.

```json
"recommendationProfiles": [{
  "id": "minimal-pilot",
  "title": "Minimal pilot",
  "description": "Disable optional service integrations. Baseline identity permissions still apply.",
  "values": {
    "includeExchange": false,
    "permissionProfile": "Minimal"
  }
}]
```

Keys in `values` are declared configuration **field IDs**, not environment variable names. Values must match field types and allowed options. Profiles cannot set secrets, sensitive fields, or standard target/source controls. Applying a profile preserves unrelated values and invalidates previous wizard review. Administrators can adjust the resulting choices before preparation.

## External catalog sidecars

Each external catalog entry with a `manifest` sidecar must also declare `manifestSha256`. Calculate it from the exact UTF-8 file bytes that will be served, including whitespace and line endings; publishing a reformatted file requires a new digest.

```powershell
(Get-FileHash -LiteralPath ./manifests/sample.json -Algorithm SHA256).Hash.ToLowerInvariant()
```

The GUI checks the digest before parsing and retains the accepted contract for preparation. The sidecar must remain on the catalog's exact HTTPS host and port. A digest without a sidecar is invalid. Bundled manifests and manifests embedded in an inspected local or pinned GitHub template do not use this external-sidecar field.

This establishes consistency with the catalog the administrator reviewed. It does not authenticate the publisher or prove that sidecar declarations agree with the repository's implementation. Review the immutable template commit and its hooks. Old external contracts without this digest require a fresh catalog review.

## Interactive hooks

Declare `requiresInteractiveTerminal` when lifecycle scripts require prompts. Local and direct GitHub inspection also detect interactive project, service, and infrastructure-layer hooks from parsed YAML and preserve that requirement when the project is reopened. The GUI routes these templates to approved preparation and an external terminal; it does not treat terminal launch as successful deployment.
