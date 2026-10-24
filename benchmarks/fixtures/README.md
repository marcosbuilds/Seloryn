# Benchmark fixtures

The suite deliberately keeps its private fixtures small and deterministic.
The MLP and language-model cases use the checked Seloryn projects under
`examples/projects/`; the two standalone tensor/error fixtures here exercise
the current CPU runtime directly. No fixture requires network access or a
third-party Python package.
