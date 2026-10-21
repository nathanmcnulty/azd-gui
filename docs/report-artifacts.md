# Read-only deployment report artifacts (GUI-010 / issue #55)

This is the first bounded report-viewing slice, based on the confined native
file reader in provenance draft #164. It does not close #55. Verification
checklists, typed actions (#157), and producer-bound freshness evidence remain
separate work. No catalog consumer is opted in by this patch.

## Reviewed declaration

A saved, reviewed template manifest may add:

```json
{
  "postDeployment": {
    "schemaVersion": "1.0",
    "artifacts": [{
      "id": "plan",
      "label": "Change plan",
      "path": "reports/plan.json",
      "format": "json",
      "stage": "plan",
      "required": true,
      "sensitive": true
    }]
  }
}
```

The desktop reads declarations from the registered project's persisted reviewed
snapshot, not from a caller-supplied path or a mutable workspace manifest. A
caller may request an artifact ID for reveal, paired with the currently reviewed
snapshot digest. A changed snapshot rejects that reveal. Missing contracts do
not produce a report panel. No artifact is a deployment prerequisite in this
slice; `required` is informational metadata, not proof of a successful stage.
Unknown contract fields (including commands or verification actions), versions,
formats and stages reject rather than acquiring behavior.

## File and display boundary

Only relative paths below `reports/` are accepted. Dot-prefixed segments,
traversal, absolute paths, backslashes, alternate streams, control characters,
empty segments, Windows device names and trailing-dot/space aliases are rejected
by the native validator. The registered project is matched before any filesystem
resolution. Existing no-follow/anchored native access rejects symlinks, reparse
points and nonregular files. The Windows implementation still requires native
runtime acceptance; Linux source inspection is not that evidence.

Limits are 32 declarations, 64 KiB per file, 1,024 UTF-8 path bytes, 64 path
segments, 64-byte IDs and 160-byte labels. JSON must parse under serde_json's
normal depth limit. All formats require valid UTF-8 and reject control characters
other than tab, CR and LF. JSON/text/Markdown are rendered as escaped, inert text.
Markdown HTML, hyperlinks, images and scripts are not activated. No renderer
network requests, shell commands, file writes, URLs, telemetry, indexing or
console logging are added.

Sensitive files are not opened, hashed or returned until the user presses their
explicit reveal button. Reveals are not persisted. Refresh, hide, project or
environment/revision/operation changes, reviewed snapshot updates, connection
context changes, saved/draft configuration changes, and output changes clear all
displayed payloads immediately;
late responses for a superseded view are ignored. A refresh hides sensitive
content again. No exception detail or raw filesystem path is placed in the error
message. As with any application displaying a secret, a deliberate reveal makes
that content visible in the local UI and process memory; it is not a secret vault.

## Evidence semantics

Available, hidden, missing, unsafe-path, oversized, invalid and unreadable states
are separate. The reader returns the selected environment/source revision,
reviewed snapshot digest, observation time, file modification time and hash of
observed bytes. These are **observation bindings**, not proof of the artifact's
producer, tenant, configuration, deployment generation or successful operation.
Modification time can be copied or forged. Every displayed report retains an
explicit "verification unproven" warning. A report containing `passed`, a
command, or a suggested permission does not grant authority or change state.

The UI discards stale *displayed content* when its project context changes. This
slice does not classify an old on-disk artifact as current: freshness remains
unproven even if modification time is recent. Full producer/tenant/configuration
binding, continuous detection of external file edits, and cross-context evidence
correlation are deferred. Operators must refresh after external changes. This
limitation must stay visible rather than suggesting a stale report has passed.

## Validation and remaining gates

- Frontend tests cover optional absence, inert rendering, reveal/hide, metadata
  and path rejection, late replies, context changes and generic errors.
- Native tests are supplied for bounded content, malformed data, sensitive
  non-reading, metadata, symlink escapes and observation context.
- Run `npm run verify` with the repository's Rust toolchain; the resumed
  cloud workspace did not have `cargo`, so no native compile/test/format/Clippy
  pass is claimed for this patch. Node tests are not a substitute.
- Windows no-follow/reparse behavior and visual acceptance need a dedicated
  native session. No Azure account, lab resource, authentication or deployment
  is needed to complete these local checks.
- No dependency changes, installation, hosted workflow dispatch, publication,
  merge, deployment or new permission occurred.
