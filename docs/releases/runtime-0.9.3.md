# Seloryn runtime 0.9.3 / language 0.09

This local runtime patch closes the installation-integrity gap left by the
provenance staging work. The Windows installer can now consume the staged
`seloryn.provenance.v1` record and rejects an unsupported schema, compatibility
version mismatch or binary SHA-256 mismatch before it copies a binary into the
installation root.

The installer probe covers a valid provenance record and an intentionally
tampered record, in addition to install, upgrade, rollback and uninstall. This
is still a local `verified-preview` candidate: provenance is not a cryptographic
signature, the ABI remains unstable and no public release was created.
