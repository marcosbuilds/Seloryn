# Test layout

- `bootstrap_cli.rs` covers command parsing, diagnostics, channels, projects
  and bundles.
- `tensor_cpu.rs` covers the end-to-end CPU tensor bridge and negative limits.
- `n1_probe.py` and `n2_probe.py` are independent standard-library oracles;
  they compare real CLI output and do not execute Seloryn behind the scenes.
- `n3_probe.py` independently computes scalar derivatives with finite
  differences and compares them with the real scalar reverse-mode CLI gate.
- `n3_tensor_probe.py` compares a real Tensor[F32] reverse-mode result with an
  independent matrix/mean derivative oracle.
- `release_probe.py` executes a typed tensor-index example, public projects and
  a bundle with declared JSON data on the locally built release binary.
- `import_contract_probe.py` proves that every symbol in a local multi-symbol
  import is checked and that missing exports fail closed with `SEL-M004`.
- `structured_inspection_probe.py` proves the compact `seloryn.analysis.v1`
  project/source context, local/standard import classification and negative
  diagnostics/exit behavior without executing the fixture.
- `adversarial_stream_probe.py` proves bounded receipt-verifier handling for
  valid Unicode/extreme numbers, non-finite numbers, truncation, duplicate
  keys, tamper, invalid Unicode/UTF-8, nesting and line-size limits.
- `performance_probe.py` runs the source-owned training fixture repeatedly and
  reports median/P95 wall time alongside the pure-Python numerical oracle.
- `../scripts/ci/conformance.py` validates the public capability matrix and
  runs the existing probes by evidence suite; its source of truth is
  `conformance_matrix.json`.

Only sanitized public fixtures that are executable against this checkout belong
here.
