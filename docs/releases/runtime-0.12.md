# Seloryn runtime 0.12.0 / language 0.12

This verified-preview line generalizes the source-owned language-model proof.
The `.sely` project now loads vocabulary and dataset sizes from JSON, exposes
reusable `Parameter` and `Sgd` modules, uses fused cross-entropy with reverse
autodiff, and proves a second five-token vocabulary without source changes.

The release evidence also includes a real Windows terminal installation with
per-user PATH integration, CMD/PowerShell execution, and a verified JSON2
stream from the installed binary.

Evidence:

- [`examples/projects/self-hosted-language-model`](../../examples/projects/self-hosted-language-model)
- [`tests/self_hosted_language_model_probe.py`](../../tests/self_hosted_language_model_probe.py)
- [`tests/compare_language_model.py`](../../tests/compare_language_model.py)
- [`tests/installer_probe.ps1`](../../tests/installer_probe.ps1)
- [`docs/compute/language-model.md`](../compute/language-model.md)

The runtime remains `verified-preview` with an unstable ABI. This is a bounded
CPU contextual linear model with one-hot windows and whitespace tokenization,
not an embedding model, transformer, subword tokenizer, GPU tensor trainer,
HTTP server or stable production runtime.
