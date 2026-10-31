# Executable language-model proof

Seloryn now has two complete, bounded contextual next-token workflows authored in
`.sely`. The fixture uses a runtime-selected context window (the proof uses four
tokens), an owned dataset and either a one-hot linear softmax model or a
selectable embedding/ReLU MLP model. The paths are real: source modules load
`vocab.json` and token datasets, derive tensor dimensions, compute fused or
indexed cross-entropy, request reverse-mode gradients, update named parameters
with reusable SGD, evaluate held-out loss/perplexity, persist JSON checkpoints
with SHA-256 metadata for every parameter and the vocabulary, reload them and
answer token-list or free-text CLI requests. The one-hot source also passes a
five-token vocabulary variant.

## Project tree

```text
self-hosted-language-model/
├── seloryn.toml
├── data/
│   ├── vocab.json
│   ├── train.json
│   └── test.json
├── checkpoints/
│   └── .gitkeep
└── src/
    ├── self-hosted-language-model-entry.sely
    └── app/
        ├── checkpoint.sely
        ├── data.sely
        ├── model.sely
        ├── optim.sely
        ├── parameters.sely
        ├── production.sely
        ├── serving.sely
        ├── tokenizer.sely
        └── training.sely
```

The import graph is intentionally ordinary Seloryn modules:

```mermaid
flowchart LR
    D[JSON dataset] --> T[app.tokenizer]
    T --> DA[app.data]
    DA --> M[app.model]
    M --> TR[app.training]
    TR --> C[app.checkpoint]
    C --> S[app.serving]
    S --> P[app.production]
    P --> CLI[stdin/stdout JSON]
```

There is no call to a predefined `train_mlp`, `train_linear` or prediction
function in the project source. The runtime provides tensor primitives, fused
cross-entropy, indexed targets, embedding lookup, reverse-mode gradients and
bounded whitespace splitting; the architecture, context-window construction,
objective, loop, evaluation, serialization and inference orchestration are
`.sely` code.

## Verified embedding/ReLU MLP path

`train_embedding` selects a separate CPU architecture without changing the
original one-hot checkpoint path:

```text
Tensor[I64, rows x context]
    -> embedding table [vocabulary x D]
    -> flatten(rows, context x D)
    -> dense + ReLU
    -> dense logits
    -> indexed cross-entropy
```

The proof uses vocabulary `14`, context `4`, embedding dimension `8`, hidden
width `32`, seed `17` and `160` full-batch SGD updates. It checks repeated-index
scatter-add, autodiff parity, per-tensor checkpoint hashes, fresh-process
inference, deterministic repeatability, exact `80 + 80` resume, and rejection
of changed vocabulary/dataset/state. The independent stdlib reference matches
final and held-out loss/perplexity within `2e-4`, with equal accuracy and
generated text. Measured values are initial loss `2.639393568`, final loss
`0.148477018`, held-out loss `0.838562548`, perplexity `2.313039780` and
accuracy `0.666667` on this fixture.

## Reproduction

From the repository root, build the release binary and run the black-box proof:

```powershell
cargo build --release --locked
python tests/self_hosted_language_model_probe.py
python tests/compare_language_model.py
```

The proof also executes negative paths for an unknown token, malformed dataset,
short prompts, duplicate vocabulary entries and a reordered vocabulary with the
same size. It then copies the project, changes the vocabulary to five tokens and
trains that variant without source changes. Training emits 21
sampled `training.progress` events plus source-owned contract/completion records
inside a 25-event hash-chained JSON2 receipt, and the receipt is replayed in an
isolated copy. The final checkpoint is loaded by a fresh inference process, not
read from the training object in memory.

## Measured comparison

The original comparison loads the same vocabulary and datasets, then uses the
same four-token one-hot window, full-batch linear model and 160 updates in
Python's standard library. The embedding proof has a separate pure-Python
reference. These are correctness comparisons. Process and filesystem
noise is material at this size; the measurement is evidence of current
overhead, not a speed claim.

| implementation | initial loss | final loss | held-out loss | perplexity | accuracy | generation | wall time |
|---|---:|---:|---:|---:|---:|---|---:|
| Python stdlib | 2.639057330 | 0.181598135 | 1.088078311 | 2.96856393 | 0.666667 | `in in park <eos>` | `0.259984 s` median of 3 local samples |
| Seloryn `.sely` | 2.639057398 | 0.181598127 | 1.088078260 | 2.96856380 | 0.666667 | `in in park <eos>` | `0.878805 s` median of 3 local samples |

The exact wall time varies by machine. The acceptance invariant is numerical:
final-loss and held-out-loss differences are below `1e-5`, and accuracy is
equal. The model's context window, vocabulary and tensor dimensions are derived
from the request and dataset rather than fixed in source. `infer_text` splits
Unicode whitespace and still requires every resulting token in the vocabulary.
The checkpoint binds the exact vocabulary hash, so reordering tokens with the
same size is rejected. The embedding path is still not a transformer, subword
tokenizer, GPU trainer, HTTP server or stable production runtime.

As a separate cold-process observation on the Windows release binary, three
free-text inference runs took `0.1325 s`, `0.0903 s` and `0.0819 s`. This is
specific to this fixture, host and process boundary; it records the current
overhead and is not a general throughput or latency guarantee.

The complete hashes, exit codes, replay result and receipt details are recorded beside the
project in [`evidence.md`](../../examples/projects/self-hosted-language-model/evidence.md).
