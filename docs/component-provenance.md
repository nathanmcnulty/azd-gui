# Local shared component provenance (GUI-007 / issue #58)

## Scope and compatibility

This first slice reads the optional project-root `azd-components.lock.json` and
shows installed versions, declared source repositories and immutable revisions,
managed paths and SHA-256 hashes, and local integrity status in the project
workspace. The source is Reference's existing **1.0** contract, pinned at
[`141b158f6e678ec6d7acf5c2a0c6e7d7e54670ea`](https://github.com/nathanmcnulty/azd-reference/blob/141b158f6e678ec6d7acf5c2a0c6e7d7e54670ea/schemas/azd-components-lock.schema.json).
No new lock format or dependency on Reference availability is introduced.

Only an exact stored project path can invoke the native reader; registration is
checked before filesystem resolution. The UI loads it
independently, with native hashing on a blocking worker. No lock means no new
panel. Read failures and unsupported metadata remain informational; discovery,
initialization, preview, and deployment do not consume these results as gates.
The browser-only preview returns the absent state rather than simulating native
filesystem validation.

The panel displays source strings as escaped, inert text. It neither navigates
to declared repositories nor offers executable metadata actions. Files outside
the lock are not scanned or assigned ownership. In particular, solution-owned
extensions are outside this read-only check.

## Status semantics

- **Verified:** all declared managed file bytes match the lock's SHA-256 hashes.
  This is a local consistency check, not source authentication, release
  attestation, or evidence that the version is current upstream.
- **Modified:** at least one declared file differs from its expected hash.
- **Missing:** at least one declared file is absent.
- **Unsupported:** a safe verification cannot be completed (unsupported contract,
  invalid metadata, unsafe path, read failure, or exceeded safety limit).

Every file has its own status. A component's summary prioritizes unsupported,
then missing, then modified, so incomplete validation cannot appear verified.
Unknown fields in otherwise valid 1.0 metadata are ignored and labeled as
unsupported additional metadata; they are neither returned verbatim nor
interpreted as paths, commands, links, or ownership declarations. An unknown
manifest version is displayed without interpreting its components.

## Local read boundary

The reader does not write files, load PowerShell, execute project code, fetch
remote content, or invoke a sync tool. It reads only the fixed lock filename and
the explicitly declared target paths. Source paths are validated as metadata;
they are never opened locally.

Limits per inspection:

- 1 MiB lock, 128 components, 2,000 total file records
- 1,024 UTF-8 bytes and 64 segments per relative path
- 16 MiB per managed file and 64 MiB total hashed bytes
- bounded version/repository metadata and the JSON parser's depth limit

Relative paths reject traversal, absolute/drive/UNC paths, alternate data stream
syntax, backslashes, control characters, empty segments, trailing-dot/space
aliases, and Windows device names. Duplicate component IDs and case-insensitive
ASCII duplicate target declarations are unsupported.

On Unix, directory-descriptor-relative `openat` with `O_NOFOLLOW` anchors every
lookup, including project-root ancestors, and rejects links in all segments. Nonblocking opening plus regular-file
checks avoids FIFO/device reads. On Windows, opened directory handles deny
delete sharing while descendants are opened; every ancestor and final opened
handle is inspected for reparse points. Only local drive roots are supported.
The lock and target files must be regular files. Windows junction behavior still
requires native Windows execution of its conditional fixture.

## Validation and rollout gates

The native fixture pins Reference's valid 1.0 test lock. Additional offline tests
cover exact binary hashes, drift, missing files, no mutation, unknown/future
metadata, malformed declarations, budgets, traversal, aliases, duplicate records,
links, nonregular files, and root replacement. UI tests cover all statuses, exact
provenance, inert metadata, optional absence, refresh, project switching,
late completions, and dismissal.

Run the repository's existing `npm run verify` and dependency audits. The pure
reader can also be compiled in an isolated harness for focused Unix tests, but
that does not substitute for the repository integration suite. Hosted Actions
remain manual-only under [the Actions usage policy](actions-policy.md); ordinary
pushes and draft PRs should not schedule a workflow.

This remains a reviewable draft until independent review and applicable checks
complete. Clean-VM Windows visual/native acceptance stays separate. No live
Azure validation, authentication, permissions, signing, release, or deployment
is needed for this slice.

Remote update availability is [#158](https://github.com/nathanmcnulty/azd-gui/issues/158).
Explicitly trusted sync is [#159](https://github.com/nathanmcnulty/azd-gui/issues/159).
This PR does not close those follow-on issues or claim all of GUI-007 is done.
