# Next executable gates

Seloryn is currently `0.06`: a bounded verified-preview language and runtime for
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
  capability-aware hardware gate with synchronized WGSL readback, while model
  training remains CPU-only.
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
  schema-v1 JSON remains available for compatibility.

## Next gates

1. **Editor distribution — completed:** the public `vscode-v0.4.0` release
   contains a deterministic self-contained VSIX, the Seloryn mark and the
   optional `.sely` file-icon theme. A real VS Code host still needs an
   interactive rendering check; that is a validation task, not an unbuilt
   distribution feature.
2. **Editor/runtime integration — saved-file bridge completed:** the editor now
   uses the CLI's structured diagnostics as its source of truth, with separate
   arguments, workspace-root selection, CRLF/Unicode fixtures, cancellation,
   timeout and output bounds. A full LSP, unsaved-buffer analysis and broader
   project diagnostics still require their own contracts and tests.
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
6. **Receipt continuation:** add dataset/program/result identity to the run
   receipt, then prove resume equivalence, error receipts, slow/disconnected
   consumers and measured overhead. Replay, signatures, redaction and general
   execution events must each earn separate evidence; they are not implied by
   JSON2.

## Boundaries

No current gate implies a package registry, unrestricted effects, GPU tensor
training, distributed execution, general tensor autodiff, model serving, a
full LSP or a stable language release. See [current status](status.md) for the
observed implementation boundary.
