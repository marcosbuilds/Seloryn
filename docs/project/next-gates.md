# Next executable gates

Seloryn is currently `0.09`: a bounded verified-preview language and runtime for
AI development and integration. The next steps are executable gates, not a
promise that every design proposal already exists.

## Completed baseline

- The public repository has a deterministic front end, pure bounded execution,
  local project/bundle support and bounded N1/N2 CPU tensor behavior.
- Tensor[F32] reverse-mode autodiff is executable for one input and a rank-zero
  CPU objective, with an independent CLI oracle.
- A bounded two-layer CPU MLP now trains on real JSON data with deterministic
  mini-batches, holdout evaluation, checkpoint/resume and live terminal events;
  a general model API, optimizer family and accelerator path remain outside
  the gate.
- A Seloryn-authored project now defines a tensor objective, calls gradients
  for multiple parameters, iterates with `range` and performs SGD updates in
  `.sely`; the native backend supplies the bounded tensor/autodiff kernels.
- MLP resume now detects tampering in sampler state as well as weights and enforces a
  total sample-update budget. `devices` and `gpu-probe` provide a real
  capability-aware hardware gate with synchronized WGSL readback. The closed
  scalar `train_linear_gpu` profile now performs a real device-resident update;
  MLP, Tensor and autodiff training remain CPU-only.
- Public CI exercises Linux, Windows and macOS, while independent CLI probes
  compare selected tensor results with external references.
- Project manifests now use a real TOML parser and reject malformed or
  future-language declarations before source loading; the independent
  `project_manifest_probe.py` and editor CLI fixture cover both paths.
- The VS Code package provides `.sely` association, syntax highlighting,
  native completion and opt-in model ghost text. Its installable preview is
  validated separately from the language runtime.
- `--log=json2` now emits a real hash-chained run envelope for the two bounded
  CPU training profiles. An independent Python verifier and black-box probe
  detect changed, missing, reordered, truncated and duplicate-key records;
  schema-v1 JSON remains available for compatibility. JSON2 also carries
  program/manifest/dataset/checkpoint/result identity; the public replay helper
  re-executes successful receipts in an isolated copy, signed receipts use
  keyed HMAC-SHA256 and structured path fields are redacted.
- `train_linear_gpu` is an explicit scalar F32 GPU profile: the shader updates
  device-resident parameters and performs one final readback. It fails without
  a physical adapter and never silently falls back to CPU.
- Bounded JSON numeric arrays now convert to typed lists through
  `json.as_i64_list` and `json.as_f64_list`, with positive and negative
  black-box evidence in `json_array_probe.py`.
- A bounded CPU MLP checkpoint can now be consumed by `training.predict_mlp`;
  the independent inference probe checks deterministic predictions, checkpoint
  and input hashes, JSON2 receipt verification and invalid dimensions/hash
  rejection.

## Next gates

1. **Editor distribution — completed locally:** the `0.9.0` package is on the
   same compatibility line as language `0.09` and runtime `0.9.0`. It contains
   a deterministic self-contained VSIX, the canonical Seloryn symbol and the
   optional `.sely` file-icon theme. A real VS Code host still needs an
   interactive rendering check; that is a validation task, not an unbuilt
   distribution feature.
2. **Editor/runtime integration — full-document LSP slice completed:** the
   editor now uses `seloryn lsp --stdio` by default for unsaved diagnostics,
   completion and definition, with a tested full-text synchronization client.
   `seloryn.lsp.enabled=false` preserves the saved-file CLI bridge with its
   timeout, output bound and cancellation fixtures. Incremental/indexed project
   analysis and rename support remain the next editor gates.
3. **AI workflow evidence:** compare one small Seloryn AI-development workflow
   with an equivalent conventional workflow. Measure invalid states,
   development/production adaptations and diagnostic effort before making an
   advantage claim.
4. **Numerical continuation:** turn the proven Seloryn-authored loop into
   reusable `Parameter`/`Module`/`Dataset`/`Optimizer` APIs with explicit state
   and checkpoint compatibility. The current loop proves ownership of the
   orchestration, not yet a public object framework.
5. **Runtime release:** publish supported binaries, checksums, provenance,
   upgrade/rollback behavior and a release only after the corresponding
   platform and failure-path evidence exists.
6. **Receipt continuation — first slice completed:** identity, successful
   isolated replay, structured path redaction and keyed HMAC authentication
   are now tested. Remaining gates are pre-run failure receipts, initial
   checkpoint snapshots for resume replay, public-key signatures, async
   backpressure, slow/disconnected consumer evidence and measured overhead.
7. **GPU continuation — first slice completed:** the closed `train_linear_gpu`
   proof is real on a declared physical adapter. The next gate is a reusable
   device-resident Tensor/F32 abstraction, then GPU autodiff and MLP training;
   no fallback or distributed claim is implied.

## Boundaries

No current gate implies a package registry, unrestricted effects, GPU tensor
training, distributed execution, general tensor autodiff, model serving, a
full LSP or a stable language release. See [current status](status.md) for the
observed implementation boundary.
