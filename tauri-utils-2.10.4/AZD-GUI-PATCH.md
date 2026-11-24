# Tauri Utils 2.10.4

This is the crates.io package whose SHA-256 is
`28d1d33d4453c5df0bf1672dbc7ada69d8ed74f56a4316d964a4aa73d4ecd3ad`.
The package manifests and source are provided under the accompanying licenses.
The only change from that archive is to build serde_with with its macros
feature and without its default features in the Cargo manifest. Tauri Utils
uses its skip_serializing_none attribute. This removes chrono from the
resolved lockfile.

cargo-audit skips path dependencies when checking the registry identity.
The application lock is still audited in full with denied warnings. The
supplemental lock names the original crates.io package so cargo-audit also
checks its registry advisories and yank status. The supplemental lock is
solely an audit input and is never used to build the app.

When the matching upstream fix ships, remove the local Cargo patch and crate,
resolve the registry version, and remove the supplemental lock and audit.
