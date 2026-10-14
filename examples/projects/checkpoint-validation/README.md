# Checkpoint schema validation

This project exercises the narrow `training.load_checkpoint` fixture. It
validates a bounded JSON checkpoint schema and returns the parsed document.
It does not resume training, verify cryptographic integrity, restore optimizer
state, or claim a general checkpoint format.

Run it from a development build:

```text
seloryn check examples/projects/checkpoint-validation
seloryn run examples/projects/checkpoint-validation
```
