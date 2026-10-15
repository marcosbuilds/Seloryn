# Next executable gates

Seloryn is currently `0.01`: a bounded experimental language and runtime for
AI development and integration. The next steps are executable gates, not a
promise that every design proposal already exists.

## Completed baseline

- The public repository has a deterministic front end, pure bounded execution,
  local project/bundle support and bounded N1/N2 CPU tensor behavior.
- Tensor[F32] reverse-mode autodiff is executable for one input and a rank-zero
  CPU objective, with an independent CLI oracle; optimizers and general
  trainers remain outside the gate.
- Public CI exercises Linux, Windows and macOS, while independent CLI probes
  compare selected tensor results with external references.
- The VS Code package provides `.sely` association, syntax highlighting,
  native completion and opt-in model ghost text. Its installable preview is
  validated separately from the language runtime.

## Next gates

1. **Editor distribution:** build and inspect a self-contained VSIX, keep the
   Seloryn mark on the extension, and provide an optional file-icon theme for
   `.sely`. The real VS Code host still needs an interactive rendering check.
2. **Editor/runtime integration — saved-file bridge completed:** the editor now
   uses the CLI's structured diagnostics as its source of truth, with separate
   arguments, workspace-root selection, CRLF/Unicode fixtures, cancellation,
   timeout and output bounds. A full LSP, unsaved-buffer analysis and broader
   project diagnostics still require their own contracts and tests.
3. **AI workflow evidence:** compare one small Seloryn AI-development workflow
   with an equivalent conventional workflow. Measure invalid states,
   development/production adaptations and diagnostic effort before making an
   advantage claim.
4. **Numerical continuation:** define tensor autodiff contracts and an
   independent numerical oracle before calling training general. The current
   scalar gradient and linear-training fixtures remain narrow.
5. **Runtime release:** publish supported binaries, checksums, provenance,
   upgrade/rollback behavior and a release only after the corresponding
   platform and failure-path evidence exists.

## Boundaries

No current gate implies a package registry, unrestricted effects, GPU,
distributed execution, general tensor autodiff, model serving, a full LSP or a
stable language release. See [current status](status.md) for the observed
implementation boundary.
