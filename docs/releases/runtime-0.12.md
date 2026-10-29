# Seloryn runtime 0.12.0 / language 0.12

This verified-preview line generalizes the source-owned language-model proof.
The `.sely` project now loads vocabulary and dataset sizes from JSON, exposes
reusable `Parameter` and `Sgd` modules, uses fused cross-entropy with reverse
autodiff, and proves a second five-token vocabulary without source changes.

The runtime release contains only the language binary and runtime provenance.
The Windows installer is a separate unsigned release asset; its installation
lifecycle is maintained by the private distribution pipeline.

Evidence:

- [`examples/projects/self-hosted-language-model`](../../examples/projects/self-hosted-language-model)
- [`tests/self_hosted_language_model_probe.py`](../../tests/self_hosted_language_model_probe.py)
- [`tests/compare_language_model.py`](../../tests/compare_language_model.py)
- [`tests/release_staging_probe.py`](../../tests/release_staging_probe.py)
- [`packages/catalog.toml`](../../packages/catalog.toml)
- [`docs/compute/language-model.md`](../compute/language-model.md)

The release staging probe reads the current versions from `VERSION.toml`,
selects the native host target, rejects missing or platform-divergent binaries,
and validates the runtime ZIP manifest, commit, hashes and byte counts. It also
keeps hash tampering as a failing case. This is reproducible local provenance
evidence; the runtime remains `verified-preview` with an unstable ABI.

This is a bounded
CPU contextual linear model with one-hot windows and whitespace tokenization,
not an embedding model, transformer, subword tokenizer, GPU tensor trainer,
HTTP server or stable production runtime.
