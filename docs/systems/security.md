# Security commitments

This design follows least privilege, input validation, proven cryptography and transparent threat modeling. The runtime is **not** automatically a sandbox.

| Threat | Design response |
|---|---|
| Malicious imported package | Lockfile integrity, signing/provenance review, no implicit remote execution |
| Token leaks | Never print authorization headers; redact logs and crash reports |
| SSRF and redirects | Network destination policy, revalidate resolved IP/redirects, local-network protections |
| Untrusted model files | Validate structured weights, metadata and size before allocation; never deserialize code |
| GPU/FFI memory faults | Explicit ABI/lifetime contracts, fuzz malformed buffers, provider isolation where possible |
| Command injection | Argument vectors by default, explicit unsafe shell execution only |
| Denial of service | Max input bytes, tensor allocation limits, timeouts, concurrency and output budgets |
| Supply chain | Pinned dependencies, auditable licenses, reproducible builds and artifact checksums |

The project must publish advisories after coordinated fixes without copying secret material into public issues. No guarantee of safe execution of arbitrary programs is implied.
