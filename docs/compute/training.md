# Model training

Seloryn's goal is to train actual networks end-to-end, not just call external inference endpoints. This requires a reliable stack from dataset ingestion to optimizer checkpointing.

### Intended training loop (unimplemented)

```seloryn
from seloryn.nn import Linear, ReLU, Sequential, CrossEntropy
from seloryn.optim import AdamW

let network = Sequential([
    Linear(784, 256),
    ReLU(),
    Linear(256, 10)
])
let optimizer = AdamW(network.parameters(), rate=0.0003)

for batch in dataset.batches(size=64):
    optimizer.zero_grad()
    using tape = GradientTape():
        let logits = network(batch.images)
        let loss = CrossEntropy(logits, batch.labels)
    tape.backward(loss)?
    optimizer.step()?
    metrics.record("loss", loss.item())
```

The final optimizer, gradient-tape and layers APIs are candidates until fully tested; example is a **design target**. There is no existing trainer or dataset API yet.

### Minimum viable REAL training evidence

- Create a small labeled dataset and deterministic data loader with seed and full batching semantics.
- Train a linear model with verified loss decrease, numeric gradient check and correctness against analytic reference.
- Train a multi-layer network on CPU; demonstrate fit on a nontrivial dataset and repeatable evaluation.
- Save and reload model **and optimizer** state, seed/RNG state, scheduler, progress and schema version; resumed training reproduces the expected trajectory within documented tolerance.
- Handle interrupted writes atomically; reject malformed/untrusted checkpoint headers; cap decompression and allocations.
- Report actual dataset, steps, seed, model, dtype, device, memory and timings. Never fabricate benchmark results.

### Success criteria and scope

A custom attention block and tokenizer are later stages. Training a large language model requires fast GEMM/attention kernels, reliable memory planning, high-bandwidth device execution, mixed precision, reproducibility, optimizer state sharding, distributed collectives and huge curated datasets. Neither language elegance nor its initial interpreter can substitute for these requirements.
