# Seloryn runtime 0.05

This verified-preview runtime release contains the strict project-manifest
gate introduced by `N7-STRICT-PROJECT-MANIFESTS`. `seloryn.toml` is parsed as
TOML with a closed typed schema; malformed syntax, wrong field types, unknown
fields and a language version newer than the runtime fail before source
loading. The release archive also contains a machine-readable provenance
manifest and the GitHub release publishes SHA-256 checksums.

The release does not claim a stable ABI, GPU tensor training, distributed
execution, a package registry, an HTTP runtime or a general model framework.
The VS Code extension remains `0.4.0`: no separate editor behavior was
credited by this runtime-only gain.
