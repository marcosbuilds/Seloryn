# Executed evidence

Commands:

```text
cargo build --release --locked
python tests/self_hosted_language_model_probe.py
python tests/compare_language_model.py
```

The probe runs the release binary directly. It checks this project, trains from
`{"mode":"train","context_size":4,"steps":160}`, verifies the JSON2 receipt, starts a second CLI process
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
| final train loss | `0.18159812688827515` |
| test loss | `1.088078260421753` |
| test perplexity | `2.9685637950897217` |
| reloaded test loss | `1.088078260421753` |
| test accuracy | `0.6666666666666666` |
| comparison benchmark | Python median `0.360989 s`; Seloryn median `1.062518 s`, 3 cold process runs |
| generated text from default `<bos>` run | `in in park <eos>` |
| train dataset SHA-256 | `a2b046c15edeaf917f4d6cfb614f7ba5c7da2bd1912dc7611b0f7793fe63a76a` |
| test dataset SHA-256 | `2dfe6adb2e29fc1cfa232adb25ff04defa6baec18cebe4a43cf446a25da9a6df` |
| checkpoint SHA-256 | `54b89f4b1fbadf45d07654d8b0cc08cd1e3f59bee4e55a09e8ca7aca4fc497bb` |
| inference result | `on the mat <eos>`, `status=ok` (four-token prompt and free-text request) |
| invalid-token result | non-zero; `unknown token in vocabulary` |
| short prompt | non-zero; `prompt must contain at least context_size tokens` |
| duplicate vocabulary | non-zero; `vocabulary tokens must be unique` |
| reordered vocabulary | non-zero; `checkpoint vocabulary hash does not match tokenizer` |
| dataset too short | non-zero; `dataset must contain more tokens than context_size` |

The stdlib-Python reference implements the same 4-token one-hot contextual
linear model, full-batch gradient update and greedy generation:

| implementation | initial loss | final loss | test loss | perplexity | accuracy | generation |
|---|---:|---:|---:|---:|---:|---|
| Python stdlib | `2.639057330` | `0.181598135` | `1.088078311` | `2.96856393` | `0.666667` | `in in park <eos>` |
| Seloryn `.sely` | `2.639057398` | `0.181598127` | `1.088078260` | `2.96856380` | `0.666667` | `in in park <eos>` |

Loss curve (steps `1,40,80,120,160`, lower is better):

```text
#################### ### # # #
```

No predefined model-training or prediction function is called by the project
sources; Rust supplies tensor kernels, fused cross-entropy and reverse-mode
autodiff primitives. The source derives the contextual matrix shape from the
vocabulary and request, passes a separate 5-token training variant, and binds
checkpoint reload to both the weight hash and exact vocabulary hash. Inference
does not reload the datasets to revalidate their source hashes.
