# Examples

Each example here is classified by the behavior it actually exercises. A
file is only described as runnable when the current CLI and tests execute it.

```text
examples/
  autograd/                   narrow scalar reverse-mode examples
  tensors/                     standalone CPU tensor programs
  projects/json-config/        local module + native json + external JSON
  projects/checkpoint-validation/ bounded checkpoint schema validation
  projects/linear-training/    narrow fixture-driven linear training contract
  projects/mlp-training/       verified bounded CPU MLP training, holdout and resume
  projects/mlp-inference/      bounded batch inference from a verified MLP checkpoint
  projects/self-hosted-linear/ Seloryn-authored tensor objective and SGD loop
  projects/evaluation-pipeline/ local modules + JSON validation + tensor metrics
```

Run a standalone tensor example with a locally built release binary:

```text
seloryn run examples/tensors/tensor-matmul.sely
seloryn run examples/tensors/tensor-index.sely
seloryn run examples/tensors/tensor-numeric.sely
seloryn run examples/tensors/tensor-reduce.sely
seloryn run examples/tensors/linear-forward.sely
```

Run a project example by passing its project directory:

```text
seloryn check examples/projects/json-config
seloryn run examples/projects/json-config
seloryn check examples/projects/evaluation-pipeline
seloryn run examples/projects/evaluation-pipeline
seloryn check examples/projects/mlp-training
seloryn run examples/projects/mlp-training --log=json
seloryn check examples/projects/mlp-inference
seloryn run examples/projects/mlp-inference
seloryn check examples/projects/self-hosted-linear
seloryn run examples/projects/self-hosted-linear
```

Run the scalar reverse-mode example:

```text
seloryn run examples/autograd/scalar-gradient.sely
seloryn run examples/autograd/scalar-gradcheck.sely
```

Run the checkpoint schema fixture:

```text
seloryn run examples/projects/checkpoint-validation
```

The project examples are deliberately small integration fixtures. They do not
claim a package registry, general model framework, unrestricted file access,
general tensor autodiff, a reusable optimizer framework or production training
support. The bounded tensor/autodiff and Seloryn-authored SGD examples are
explicitly limited to the contracts listed above.
