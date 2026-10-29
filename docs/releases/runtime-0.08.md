# Seloryn runtime 0.8.0 / language 0.08

This local candidate records the `N10-JSON-TYPED-ARRAY-INGESTION` language
gain. `json.as_i64_list` and `json.as_f64_list` convert bounded JSON numeric
arrays into typed Seloryn lists, allowing ordinary source-level iteration
without embedding the input values into `.sely` source.

Evidence:

- [`src/runtime/mod.rs`](../../src/runtime/mod.rs)
- [`src/language/semantic/checker.rs`](../../src/language/semantic/checker.rs)
- [`examples/projects/json-array-pipeline`](../../examples/projects/json-array-pipeline)
- [`tests/json_array_probe.py`](../../tests/json_array_probe.py)

The conversion rejects non-numeric array members and enforces the existing
bounded list limit. This is not a dataframe API, package registry, general
model loader or stable ABI. Runtime `0.8.0` remains on `verified-preview` until
the coordinated artifact is independently reviewed and explicitly published.
