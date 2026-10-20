# Seloryn runtime 0.11.0 / language 0.11

This local verified-preview candidate records the `N13-SELF-HOSTED-LANGUAGE-MODEL`
capability. A complete small next-token model can now be authored in modular
`.sely` source with owned data, a source-defined loss and SGD loop, held-out
evaluation, persisted checkpoint reload and a separate inference invocation.

Evidence:

- [`examples/projects/self-hosted-language-model`](../../examples/projects/self-hosted-language-model)
- [`tests/self_hosted_language_model_probe.py`](../../tests/self_hosted_language_model_probe.py)
- [`tests/compare_language_model.py`](../../tests/compare_language_model.py)
- [`docs/compute/language-model.md`](../compute/language-model.md)

The runtime remains `verified-preview` with an unstable ABI. The model is a
fixed-vocabulary bigram baseline, and the integration boundary is JSON over
stdin/stdout with a local checkpoint. No public binary release, stable ABI,
GPU language-model training or HTTP serving is claimed by this document.
