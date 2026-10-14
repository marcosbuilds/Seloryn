# Model training

**General training remains a design target:** Seloryn 0.00 does not have a
general trainer, dataset API, optimizer or model execution runtime. The
repository does contain a narrow, fixture-driven `training.train_linear`
contract with real CPU parameter updates, MSE/SGD, deterministic inputs,
holdout evaluation and bounded checkpoint-schema validation. Its output reports
`holdout_mae_max` as the enforced cap (`0.05`) and
`holdout_mae_observed` as the measured value. A holdout value above the cap is
a runtime failure and cannot be returned as a successful training result; the
fixture emits no `training.complete` event for that failed run. The `seed` is
recorded metadata, not a shuffle/RNG implementation. The separate `training.load_checkpoint`
fixture validates a bounded JSON schema and returns the parsed document; it
does not restore optimizer/RNG state, verify cryptographic integrity or resume
a real training loop. This does not imply a general ML framework. Seloryn's AI
mission is to eventually train actual networks end-to-end, not just call
external inference endpoints.

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

The final optimizer, gradient-tape and layers APIs are candidates until fully
tested; this example is a **design target**. The narrow fixture is not exposed
as a general dataset API.

### Executable narrow fixture

The public project example runs the implemented contract against JSON data:

```text
seloryn run examples/projects/linear-training
```

The observed output is:

```json
{"holdout_mae_max":0.05,"holdout_mae_observed":0.01893684,"loss_decreased":true,"seed":7}
```

This is evidence for one bounded linear problem only; it is not evidence for
MLP training, a general optimizer, checkpoint restoration or model serving.

### Executable checkpoint-schema fixture

```text
seloryn run examples/projects/checkpoint-validation
```

This fixture checks required schema and JSON value types under the project's
read and byte limits. It is not a checkpoint interchange format, integrity
mechanism or resume implementation.

### Opt-in training events

The same fixture can expose a bounded execution stream without contaminating
the program result:

```text
seloryn run examples/projects/linear-training --log=human
seloryn run examples/projects/linear-training --log=json
```

Events go to `stderr`; the final JSON result remains on `stdout`. The stream
contains `training.start`, sampled `training.step` events (first step, each
10% and final step), and `training.complete`. It identifies the linear model,
training/holdout paths and sample counts, reports measured losses and labels
the current `seed` as `metadata_only`, because this fixture does not implement
shuffle or RNG semantics. The completion event calls the measured holdout
value `holdout_mae_observed`; the program result separately reports the
enforced cap as `holdout_mae_max`. The fixture also caps total parameter
updates, in addition to dataset bytes, samples and requested steps. Logging is
off by default and this is not a general telemetry, profiler or trainer API.

### Minimum viable REAL training evidence

- Create a small labeled dataset and deterministic data loader with seed and full batching semantics.
- Train a linear model with verified loss decrease, numeric gradient check and correctness against analytic reference.
- Train a multi-layer network on CPU; demonstrate fit on a nontrivial dataset and repeatable evaluation.
- Save and reload model **and optimizer** state, seed/RNG state, scheduler, progress and schema version; resumed training reproduces the expected trajectory within documented tolerance.
- Handle interrupted writes atomically; reject malformed/untrusted checkpoint headers; cap decompression and allocations.
- Report actual dataset, steps, seed, model, dtype, device, memory and timings. Never fabricate benchmark results.

### Success criteria and scope

A custom attention block and tokenizer are later stages. Training a large language model requires fast GEMM/attention kernels, reliable memory planning, high-bandwidth device execution, mixed precision, reproducibility, optimizer state sharding, distributed collectives and huge curated datasets. Neither language elegance nor its initial interpreter can substitute for these requirements.

### Training event stream v1

The bounded fixture flushes each opt-in event as it is emitted. JSON events use
`schema_version: 1`. The start event identifies `train_linear`, the linear model,
CPU device, `f64` dtype, SGD, learning rate, parameter count, dataset paths and
sample counts. Step events report update count, loss and loss change; completion
reports initial/final loss, whether loss decreased, observed holdout MAE, total
updates and the learned weight and bias. This is a narrow execution stream,
not a general telemetry, profiler or trainer API.
