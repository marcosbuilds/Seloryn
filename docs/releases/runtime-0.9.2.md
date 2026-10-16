# Seloryn runtime 0.9.2 / language 0.09

This local runtime patch adds a reproducible release-provenance envelope to
the verified staging path. The runtime archive records `seloryn.provenance.v1`
with the release commit, source-tree hash, release profile, `Cargo.lock` hash,
toolchain hash and binary hash. The staged candidate also records extension and
runtime artifact hashes in `PROVENANCE.json`, and the release probe rejects
missing or inconsistent metadata.

This improves traceability for a future supported artifact, but it is not a
cryptographic signature, stable ABI, registry or public release.
