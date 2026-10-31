# __PROJECT_NAME__

Created by `seloryn init --template=trainable-mlp`.

This is a bounded CPU project in which the dataset contract, model, loss,
gradients, SGD update, evaluation, inference and checkpoint validation are
written in `.sely`. The runtime supplies execution, tensor bounds, JSON and
declared capabilities; it does not own the model architecture.

```text
seloryn doctor .
seloryn check .
'{"mode":"train"}' | seloryn run . --format=json --log=json2
'{"mode":"eval"}' | seloryn run . --format=json --log=json2
'{"mode":"infer","feature_names":["signal_a","signal_b","signal_c","signal_d"],"features":[0.90,0.92,0.88,0.91]}' | seloryn run . --format=json --log=json2
```

The three contract files bind dataset shape and feature order, model
architecture and optimizer limits, and the CPU/no-fallback run policy. The
checkpoint carries their hashes plus both dataset hashes, so changing a
contract or input dataset is rejected before evaluation or inference. These
SHA-256 fields provide consistency/corruption detection, not authenticated
authorship; JSON2 receipts provide the separate tamper-evident execution
chain when logging is enabled.

This starter is a verified preview for small, bounded CPU MLP workflows. It
does not claim general GPU tensors, transformer training, distributed runs,
streaming datasets or a stable ABI.
