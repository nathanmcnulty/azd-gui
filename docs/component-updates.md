# Shared component update metadata

Each solution commits its own `azd-components.lock.json`. Reference's
`tooling/Sync-AzdComponent.ps1` writes the installed version, source commit,
managed paths, and SHA-256 hashes. It and
`tooling/Test-AzdComponentDrift.ps1` validate the lock against
`schemas/azd-components-lock.schema.json` in Reference.

Publishers already declare each component in `component.json`, record changes in
its changelog, and release with a signed annotated tag named
`component/<id>/v<version>`. Follow Reference's `docs/component-releases.md` for
release review and attestation. For the update hint, add one provisional
declaration to the annotation of the next new release:

1. Write JSON matching [the provisional schema](../schemas/azd-component-update.schema.json).
   Copy the component ID and exact version from the tag. Set `breaking` to
   `true` or `false` for that release. If it breaks existing use, include a
   short reason or migration step in `migrationNotes`. For a stable tag,
   compare with the previous stable release and carry forward any breaking
   changes introduced in its release candidates.
2. Validate the declaration in PowerShell 7.6 and generate the annotation line.
   Keep the existing release annotation and include this line in the same
   annotation before creating the signed tag.

   ```powershell
   $json = Get-Content ./component-update.json -Raw
   if (-not ($json | Test-Json -SchemaFile ./schemas/azd-component-update.schema.json -ErrorAction Stop)) {
       throw 'Invalid component update declaration'
   }
   "azd-component-update: $($json | ConvertFrom-Json | ConvertTo-Json -Depth 4 -Compress)"
   ```

The [consumer fixture](../src-tauri/tests/fixtures/deployment-validation-consumer-lock.json)
uses the deployment-validation entry from azd-pim's lock at
`4c6ddfdb1cc0d5f30c9cac817a259decd181cc31`. Its installed 1.1.1
commit matches Reference's signed tag. The [publisher fixture](../src-tauri/tests/fixtures/deployment-validation-component-update.json)
shows how the 2.0.0 PowerShell 7.6 requirement would be declared. The declaration
is an example for an unpublished tag. Consumers keep their installed pin
until they intentionally adopt a reviewed release.

The wizard can show declared component versions before preparation when the app
bundles a lock for that exact reviewed catalog entry. The bundled snapshots
match their respective source locks byte for byte.

- [Deployment check](../catalog/component-locks/deployment-check.json) uses
  `examples/deployment-check/azd-components.lock.json` in Reference at
  `abc6e896620990e68360a63dd14efda66a3f75cd`.
- [Maester Automation Account](../catalog/component-locks/maester-automation.json)
  uses `automation-account/azd-components.lock.json` in azd-maester at
  `1cbaf9064a6ac4c61dd175bf2f61ca08367de0ff`.
- [Selected-role PIM pilot](../catalog/component-locks/pim-pilot.json) uses
  `azd-components.lock.json` in azd-pim at
  `48bdbadb6a02c06fb4144e616ed9562b905b36ec`. Its standalone planner shows
  these declarations when the catalog pin matches the bundled native source.

Each snapshot applies only to its catalog entry, repository, revision and path.
When refreshing a pin, review the new source lock before updating the snapshot
binding. Local file integrity and update comparisons use the prepared project's
lock.

The app currently queries only the reviewed first-party Reference repository.
It reads GitHub-verified annotated tags for availability. Startup caches the
release references. Opening a template or saved project checks the tag
details relevant to its installed pins from that cached list, within the
shared request budget. After preparation registers a destination, the wizard
shows the installed pins and file integrity for that project, including for
catalog and GitHub sources. Exact installed tags and eligible upgrades take
priority over other release history. Repeated checks reuse results until
the feed refreshes. **Check for updates** rereads the local lock and its
managed files before comparing the current installed pins. Declarations for
skipped releases can contribute warnings. Missing or unsupported declarations
leave coverage incomplete. Schema validation describes the JSON shape. Source
review, catalog inclusion, release attestation, and execution approval each
have their own controls. This provisional declaration is for internal use
before the coordinated public contract.
