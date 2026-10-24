# Seloryn Evidence Lab

The Evidence Lab is the public index for reproducible claims. It combines two
distinct activities:

- **Conformance Suite:** verifies that a bounded Seloryn capability behaves as
  documented, including negative and boundary cases.
- **Benchmark Suite:** compares equivalent bounded workloads with independent
  references and reports correctness separately from timing.

The index is intentionally a source map, not a pass certificate. A result is
accepted only when the referenced command runs on the candidate commit and the
report records its environment, version, artifact identity and limitations.

## Source map

| Evidence layer | Canonical source | What it proves |
|---|---|---|
| Capability claims | [`tests/conformance_matrix.json`](../../tests/conformance_matrix.json) | Claim-to-implementation, positive probe, negative probe, platform and limit mapping |
| Conformance runner | [`scripts/ci/conformance.py`](../../scripts/ci/conformance.py) | Validates the matrix and executes selected probes without manufacturing results |
| CI evidence | [`scripts/ci/ci_evidence.py`](../../scripts/ci/ci_evidence.py) and [workflow](../../.github/workflows/ci.yml) | Sanitized per-platform records, expected-record completeness and artifact hashes |
| Workload comparison | [`benchmarks/`](../../benchmarks/) | Six independent Python references, correctness contracts and case-local median/p95 timing |
| Agent development evaluation | [`research/agent-token-economy/`](../../research/agent-token-economy/) | Frozen tasks, visible transcripts, local BPE measurement and explicit missingness |
| Runtime receipts | [`scripts/streaming/`](../../scripts/streaming/) | Hash-chain, replay, redaction and bounded local execution evidence |

## Reproduce the current checks

From the repository root:

```text
python scripts/ci/conformance.py --validate
python scripts/ci/conformance.py --run --suite core
python scripts/ci/conformance.py --run --suite benchmark
python scripts/ci/conformance.py --run --suite editor
python scripts/ci/conformance.py --run --suite release
python scripts/ci/conformance.py --run --suite hardware
python benchmarks/test_benchmark_contracts.py
python research/agent-token-economy/runner.py validate
```

The GitHub workflow runs the core matrix on every supported runner and keeps
the sanitized records as artifacts. The benchmark report is intentionally
machine-specific and generated; its timing must be tied to the exact commit,
binary hash and environment of the artifact before it is used in a claim.

## Evidence classes

`verified-preview` means the named bounded behavior has reproducible positive
and negative evidence. `experimental-implemented` means code and tests exist,
but the stronger preview boundary is not accepted. `unsupported` means the
boundary is explicitly tested/documented. None of these labels imply general
performance superiority, provider billing accuracy, production readiness or a
stable ABI.
