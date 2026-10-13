# Test layout

- `bootstrap_cli.rs` covers command parsing, diagnostics, channels, projects
  and bundles.
- `tensor_cpu.rs` covers the end-to-end CPU tensor bridge and negative limits.
- `n1_probe.py` and `n2_probe.py` are independent standard-library oracles;
  they compare real CLI output and do not execute Seloryn behind the scenes.
- `n3_probe.py` independently computes scalar derivatives with finite
  differences and compares them with the real scalar reverse-mode CLI gate.
- `release_probe.py` executes public projects and a bundle with declared JSON
  data on the release binary.

The larger private development corpus is kept outside this public tree. Only
sanitized public fixtures that are executable against this checkout belong
here.
