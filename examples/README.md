# Examples and first supported preview workflow

Each example is classified by what its current CLI and tests actually
exercise. A file or project is called **verified preview** only when a named
black-box probe or an explicit evidence record supports that description.
These examples are not a promise of a stable ABI or production support.

## First supported workflow

[`projects/auditable-local-inference`](projects/auditable-local-inference) is
the first product-shaped preview workflow: local CPU training and inference for
a small tabular equipment-health model. It keeps the dataset, model and run
contracts separate, owns the training loop in `.sely`, writes a hash-bound
checkpoint, emits JSON2 audit evidence and rejects contract/checkpoint
tampering.

The reproducible adoption gate is:

```text
cargo build --release --locked
python tests/auditable_local_inference_probe.py
```

The gate must print `Auditable local inference proof: PASS`. It checks static
validation, source ownership, decreasing training loss, holdout evaluation,
independent Python agreement, checkpoint reload, receipt verification, isolated
replay and six negative boundary cases. See the
[conformance matrix](../docs/project/conformance.md) for the exact pass criteria
and limitations.

For direct inspection after building a local binary:

```text
seloryn doctor examples/projects/auditable-local-inference --format=json
seloryn check examples/projects/auditable-local-inference --format=json
'{"mode":"train"}' | seloryn run examples/projects/auditable-local-inference --format=json --log=json2
'{"mode":"infer","feature_names":["temperature","vibration","load","age_days"],"features":[0.90,0.92,0.88,0.91]}' | seloryn run examples/projects/auditable-local-inference --format=json --log=json2
```

The direct run creates the example checkpoint. The probe uses temporary copies,
so it is the safer command for an external verification pass.

## Example map

```text
examples/
  autograd/                           narrow scalar and tensor reverse-mode examples
  tensors/                            standalone bounded CPU tensor programs
  projects/auditable-local-inference first product-shaped, source-owned workflow
  projects/self-hosted-mlp            source-owned custom ReLU MLP proof
  projects/self-hosted-language-model source-owned CPU contextual next-token proof; configurable one-hot context windows
  projects/source-contract-reuse  experimental shared optimizer contract proof
  projects/mlp-training               bounded native CPU MLP trainer and resume fixture
  projects/mlp-inference              bounded batch inference from an MLP checkpoint
  projects/self-hosted-linear         source-owned tensor objective and SGD loop
  projects/gpu-linear-training        closed scalar F32 GPU profile; host dependent
  projects/json-array-pipeline        bounded numeric JSON-array conversion
  projects/json-config                local module plus bounded JSON read/encode/get
  projects/checkpoint-validation      bounded checkpoint schema validation
  projects/evaluation-pipeline        local modules, JSON validation and tensor metrics
  projects/linear-training            narrow fixture-driven linear-training contract
  external/python-bridge              external Python consumer of the JSON/stdio contract
```

The map distinguishes evidence slices; it is not a feature roadmap. In
particular, the GPU linear project does not make the tensor or MLP projects GPU
capable, and the language-model project does not claim embeddings, transformer,
serving, subword tokenization or long-context support.

## Standalone examples

Run these with a locally built release binary:

```text
seloryn run examples/tensors/tensor-matmul.sely
seloryn run examples/tensors/tensor-index.sely
seloryn run examples/tensors/tensor-numeric.sely
seloryn run examples/tensors/tensor-reduce.sely
seloryn run examples/tensors/linear-forward.sely
seloryn run examples/autograd/scalar-gradient.sely
seloryn run examples/autograd/scalar-gradcheck.sely
```

## Project examples

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
seloryn run examples/projects/checkpoint-validation
```

The project READMEs state the narrower contract for each fixture. No example
claims a package registry, unrestricted file access, arbitrary model loading,
general tensor autodiff, a reusable optimizer framework, HTTP serving,
distributed execution or production training support.
