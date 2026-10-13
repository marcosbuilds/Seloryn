# Examples

Each example here is classified by the behavior it actually exercises. A
file is only described as runnable when the current CLI and tests execute it.

```text
examples/
  tensors/                     standalone CPU tensor programs
  projects/json-config/        local module + native json + external JSON
  projects/linear-training/    narrow fixture-driven linear training contract
```

Run a standalone tensor example with the release binary:

```text
seloryn run examples/tensors/tensor-matmul.sely
```

Run a project example by passing its project directory:

```text
seloryn check examples/projects/json-config
seloryn run examples/projects/json-config
```

The project examples are deliberately small integration fixtures. They do not
claim a package registry, general model framework, unrestricted file access or
production training support.
