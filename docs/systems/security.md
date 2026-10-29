# Security commitments

This design follows least privilege, input validation, proven cryptography and transparent threat modeling. The runtime is **not** automatically a sandbox.

| Threat | Design response |
|---|---|
| Malicious imported package | Lockfile integrity, signing/provenance review, no implicit remote execution |
| Token leaks | Never print authorization headers; redact logs and crash reports |
| SSRF and redirects | Network destination policy, revalidate resolved IP/redirects, local-network protections |
| Untrusted model files | Validate structured weights, metadata and size before allocation; never deserialize code |
| Receipt/verifier input | The NDJSON verifier bounds each line to 1 MiB, the stream to 8 MiB and JSON nesting to 128 levels; it rejects non-finite numbers, duplicate keys, truncation, invalid Unicode/UTF-8 and hash tampering with exit code 1 and stable category messages. The bounded campaign also checks that supplied token-shaped text is not echoed. |
| GPU/FFI memory faults | Explicit ABI/lifetime contracts, fuzz malformed buffers, provider isolation where possible |
| Command injection | Argument vectors by default, explicit unsafe shell execution only |
| Denial of service | Max input bytes, tensor allocation limits, timeouts, concurrency and output budgets |
| Supply chain | Pinned dependencies, auditable licenses, reproducible builds and artifact checksums |

The project must publish advisories after coordinated fixes without copying secret material into public issues. No guarantee of safe execution of arbitrary programs is implied.
