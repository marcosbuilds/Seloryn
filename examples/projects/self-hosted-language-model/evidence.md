# Executed evidence

Commands:

```text
python tests/self_hosted_language_model_probe.py
```

The probe runs the release binary directly. It checks this project, trains from
`{"mode":"train"}`, verifies the JSON2 receipt, starts a second CLI process
for checkpoint-backed inference, and checks the bounded unknown-token failure
path.

| check | result |
|---|---|
| `check` exit code | `0` |
| training exit code | `0` |
| streaming verification exit code | `0` |
| inference exit code | `0` |
| training progress events | `160` |
| receipt events | `162` (`run.start`, 160 progress, `run.end`) |
| initial loss | `2.6390585899353027` |
| final train loss | `0.8294858932495117` |
| test loss | `1.532878041267395` |
| reloaded test loss | `1.532878041267395` |
| test accuracy | `0.5333333333333333` |
| generated text | `the park <eos>` |
| train dataset SHA-256 | `a2b046c15edeaf917f4d6cfb614f7ba5c7da2bd1912dc7611b0f7793fe63a76a` |
| test dataset SHA-256 | `2dfe6adb2e29fc1cfa232adb25ff04defa6baec18cebe4a43cf446a25da9a6df` |
| checkpoint SHA-256 | `1e7fdb7d6b1dfde6622685f238986fae02a8d911816159597f86b9d66561ed18` |
| inference result | `sat on the park`, `status=ok` |
| invalid-token result | non-zero; `unknown token in dataset` |
| prompt-count mismatch | non-zero; `prompt_count must match tokens` |

The stdlib-Python reference implements the same 14x14 bigram, full-batch
gradient update and greedy generation:

| implementation | initial loss | final loss | test loss | accuracy | generation |
|---|---:|---:|---:|---:|---|
| Python stdlib | `2.639057330` | `0.829485686` | `1.532877856` | `0.533333` | `sat on the park` |
| Seloryn `.sely` | `2.639058590` | `0.829485893` | `1.532878041` | `0.533333` | `sat on the park` |

Loss curve (steps `1,40,80,120,160`, lower is better):

```text
#################### ########## #### ## #
```

No predefined model-training or prediction function is called by the project
sources; Rust supplies only tensor and reverse-mode primitives.
