# Benchmark reports

`benchmarks/runner.py` writes `latest.json` and `latest.md` here by default.
They are local run artifacts and are ignored by Git because timestamps,
hardware and process timings are environment-specific. The JSON format is
defined by `../schema/report.schema.json`; validate a generated report with
`python benchmarks/validate_json.py` from the repository root.
