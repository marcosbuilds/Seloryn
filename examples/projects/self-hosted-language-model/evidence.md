# Executed evidence

Commands:

```text
cargo build --release --locked
python tests/self_hosted_language_model_probe.py
python tests/compare_language_model.py
```

The probe runs the release binary directly. It checks this project, trains from
`{"mode":"train"}`, verifies the JSON2 receipt, starts a second CLI process
for checkpoint-backed inference, checks malformed input paths, and copies the
project to train a five-token vocabulary variant without source changes.

| check | result |
|---|---|
| `check` exit code | `0` |
| training exit code | `0` |
| streaming verification exit code | `0` |
| inference exit code | `0` |
| training progress events | `21` (steps 1, multiples of 8, and final) |
| receipt events | `25` (`run.start`, source-owned contract, 21 progress, source-owned completion, `run.end`) |
| source-owned replay | exit `0`; isolated replay matched identity, result, checkpoint and semantic events |
| initial loss | `2.6390573978424072` |
| final train loss | `0.8294857740402222` |
| test loss | `1.5328779220581055` |
| reloaded test loss | `1.5328779220581055` |
| test accuracy | `0.5333333333333333` |
| comparison benchmark | Python median `0.173277 s` (0.1704–0.1760); Seloryn median `0.810498 s` (0.7902–0.9004), 3 cold process runs |
| generated text from default `<bos>` run | `the park <eos>` |
| train dataset SHA-256 | `a2b046c15edeaf917f4d6cfb614f7ba5c7da2bd1912dc7611b0f7793fe63a76a` |
| test dataset SHA-256 | `2dfe6adb2e29fc1cfa232adb25ff04defa6baec18cebe4a43cf446a25da9a6df` |
| checkpoint SHA-256 | `e285267993473b1c7aa69a1eb27bceee149668b72917789c94fd5f5a10e87c2c` |
| inference result | `sat on the park`, `status=ok` (separate prompt request) |
| invalid-token result | non-zero; `unknown token in vocabulary` |
| prompt-count mismatch | non-zero; `prompt_count must match tokens` |
| dataset too short | non-zero; `dataset must contain at least two tokens` |

The stdlib-Python reference implements the same 14x14 bigram, full-batch
gradient update and greedy generation:

| implementation | initial loss | final loss | test loss | accuracy | generation |
|---|---:|---:|---:|---:|---|
| Python stdlib | `2.639057330` | `0.829485686` | `1.532877856` | `0.533333` | `the park <eos>` |
| Seloryn `.sely` | `2.639057398` | `0.829485774` | `1.532877922` | `0.533333` | `the park <eos>` |

Loss curve (steps `1,40,80,120,160`, lower is better):

```text
#################### ########## #### ## #
```

No predefined model-training or prediction function is called by the project
sources; Rust supplies tensor kernels, fused cross-entropy and reverse-mode
autodiff primitives. The baseline uses 14-token/39-pair and 14-token/15-pair
fixtures, but the source derives shapes from `vocab.json` and passes a separate
5-token training variant. Inference checks the checkpoint's own weight hash but
does not reload the datasets to revalidate their source hashes.
