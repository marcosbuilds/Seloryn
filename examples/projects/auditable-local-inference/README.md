# Auditable local inference

This is the first product-shaped proof for Seloryn's intended niche: a local
AI workflow whose dataset, model contract, checkpoint and runtime must agree
before training or inference can succeed.

The model is intentionally small: a four-feature ReLU MLP trained in `.sely`
with source-owned forward pass, loss, gradients, SGD loop, evaluation,
checkpoint persistence and inference. It does not call `train_mlp`,
`predict_mlp`, `train_linear_gpu` or any Rust training function.

```powershell
seloryn doctor examples/projects/auditable-local-inference --format=json
seloryn check examples/projects/auditable-local-inference --format=json
'{"mode":"train"}' | seloryn run examples/projects/auditable-local-inference --format=json --log=json2
'{"mode":"infer","feature_names":["temperature","vibration","load","age_days"],"features":[0.90,0.92,0.88,0.91]}' | seloryn run examples/projects/auditable-local-inference --format=json --log=json2
```

The project deliberately keeps its contract files separate from source:

- `contracts/dataset.json` fixes feature order, row counts and target policy;
- `contracts/model.json` fixes architecture, optimizer and training bounds;
- `contracts/run.json` forbids device fallback;
- `data/*.json` are the immutable fixture inputs;
- `checkpoints/equipment-health.json` is generated and ignored by Git.

The negative probe mutates one boundary at a time and requires a structured
non-zero failure: feature order, feature count, dataset shape, target values,
model contract and checkpoint weights. It also compares a standalone Python
reference for the numerical result. This is a verified preview, not a claim of
stable ABI, general GPU training, transformer support or distributed execution.
