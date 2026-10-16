# Seloryn runtime 0.9.4 / language 0.09

This local runtime patch adds two production-facing contracts on the existing
language line. Built `.selyapp` artifacts now encode `seloryn.bundle.v1`, emit
`seloryn-build.json` with language/runtime/profile/size/hash metadata, and reject
tampered metadata when a build directory is executed. The LSP now tracks
document versions, rejects stale or ranged full-sync changes without mutating
state, clears diagnostics on `didClose`, supports reopening and rejects malformed
or duplicate `Content-Length` headers.

Rust, CLI, bundle, LSP, release, installer and VS Code probes cover the changes.
This remains a local `verified-preview` candidate: the runtime ABI is still
unstable and no public release was created.
