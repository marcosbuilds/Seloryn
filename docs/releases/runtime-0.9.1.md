# Seloryn runtime 0.9.1 / language 0.09

This local runtime patch keeps the language compatibility line at `0.09` and
adds operational contracts needed by real project workflows:

- `seloryn doctor [project]` performs read-only manifest, source graph,
  capability and semantic validation with a stable `seloryn.doctor.v1` JSON
  report. It explicitly reports the current `verified-preview` channel,
  `abi=unstable` and `production_ready=false`.
- `seloryn init <directory>` creates a validated `minimal` or `tabular-mlp`
  project transactionally, refuses overwrite and returns a stable
  `seloryn.init.v1` report.

The patch is not a stable ABI or public registry release. Its evidence is the
independent `tests/workspace_probe.py` plus the full public verification
script.
