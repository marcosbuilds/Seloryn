"""Independent black-box proof for the bounded CPU MLP trainer."""

from __future__ import annotations

import json
import math
import struct
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")

TRAIN_DATA = {
    "inputs": [
        [-2, -2],
        [-2, 0],
        [-2, 2],
        [0, -2],
        [0, 0],
        [0, 2],
        [2, -2],
        [2, 0],
        [2, 2],
    ],
    "targets": [[-5.5], [-1.5], [2.5], [-3.5], [0.5], [4.5], [-1.5], [2.5], [6.5]],
}
TEST_DATA = {
    "inputs": [[-1, -1], [-1, 1], [1, -1], [1, 1], [3, 0]],
    "targets": [[-2.5], [1.5], [-0.5], [3.5], [3.5]],
}


def create_project(root: Path, steps: int, seed: int = 17) -> None:
    (root / "src").mkdir(parents=True)
    (root / "data").mkdir()
    (root / "checkpoints").mkdir()
    (root / "seloryn.toml").write_text(
        """[package]
name = "mlp-probe"
language = "0.02"
source_root = "src"
entry = "main.sely"

[capabilities]
read = ["data"]
write = ["checkpoints"]

[limits]
json_max_bytes = 1048576
json_max_depth = 32
""",
        encoding="utf-8",
    )
    (root / "data" / "train.json").write_text(
        json.dumps(TRAIN_DATA) + "\n",
        encoding="utf-8",
    )
    (root / "data" / "test.json").write_text(
        json.dumps(TEST_DATA) + "\n",
        encoding="utf-8",
    )
    write_source(root, steps, seed)


def write_source(
    root: Path,
    steps: int,
    seed: int = 17,
    batch_size: int = 3,
    hidden_size: int = 8,
    learning_rate: float = 0.02,
) -> None:
    (root / "src" / "main.sely").write_text(
        f'from training import train_mlp\n\nfn main() -> Json:\n'
        f'    return train_mlp("data/train.json", "data/test.json", '
        f'"checkpoints/model.json", learning_rate={learning_rate}, hidden_size={hidden_size}, '
        f'batch_size={batch_size}, steps={steps}, seed={seed})\n',
        encoding="utf-8",
    )


MASK64 = (1 << 64) - 1
RNG_SEED_MIX = 0x9E37_79B9_7F4A_7C15
RNG_NONZERO_FALLBACK = 0xD1B5_4A32_D192_ED03


def f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def next_u64(state: int) -> tuple[int, int]:
    value = state
    value = (value ^ ((value << 13) & MASK64)) & MASK64
    value = (value ^ (value >> 7)) & MASK64
    value = (value ^ ((value << 17) & MASK64)) & MASK64
    return value, value


def next_unit(state: int) -> tuple[int, float]:
    state, value = next_u64(state)
    return state, f32(f32(value & 0xFFFF_FFFF) / f32(0xFFFF_FFFF))


def oracle_one_step(
    inputs: list[list[float]],
    targets: list[list[float]],
    seed: int,
    hidden_size: int,
    learning_rate: float,
) -> dict[str, list[float]]:
    input_size = len(inputs[0])
    output_size = len(targets[0])
    state = (seed & MASK64) ^ RNG_SEED_MIX
    state = state or RNG_NONZERO_FALLBACK
    first_scale = f32(math.sqrt(f32(f32(2.0) / f32(input_size))))
    second_scale = f32(math.sqrt(f32(f32(2.0) / f32(hidden_size))))

    first_weights = []
    for _ in range(input_size * hidden_size):
        state, unit = next_unit(state)
        first_weights.append(f32(f32(f32(unit * 2.0) - 1.0) * first_scale))
    second_weights = []
    for _ in range(hidden_size * output_size):
        state, unit = next_unit(state)
        second_weights.append(f32(f32(f32(unit * 2.0) - 1.0) * second_scale))

    order = list(range(len(inputs)))
    for index in range(len(order) - 1, 0, -1):
        state, random_value = next_u64(state)
        swap = random_value % (index + 1)
        order[index], order[swap] = order[swap], order[index]

    sample = order[0]
    pre_activation = []
    hidden = []
    for hidden_index in range(hidden_size):
        total = f32(0.0)
        for input_index in range(input_size):
            product = f32(f32(inputs[sample][input_index]) * first_weights[input_index * hidden_size + hidden_index])
            total = f32(total + product)
        pre = f32(total)
        pre_activation.append(pre)
        hidden.append(max(pre, f32(0.0)))

    output = []
    for output_index in range(output_size):
        total = f32(0.0)
        for hidden_index in range(hidden_size):
            total = f32(total + f32(hidden[hidden_index] * second_weights[hidden_index * output_size + output_index]))
        output.append(total)

    scale = f32(f32(1.0) / f32(output_size))
    output_gradient = [
        f32(f32(f32(output[index] - f32(targets[sample][index])) * f32(2.0)) * scale)
        for index in range(output_size)
    ]
    first_gradients = [f32(0.0)] * len(first_weights)
    first_bias_gradients = [f32(0.0)] * hidden_size
    second_gradients = [f32(0.0)] * len(second_weights)
    second_bias_gradients = [f32(0.0)] * output_size
    for output_index, gradient in enumerate(output_gradient):
        second_bias_gradients[output_index] = f32(gradient)
        for hidden_index in range(hidden_size):
            position = hidden_index * output_size + output_index
            second_gradients[position] = f32(hidden[hidden_index] * gradient)
    for hidden_index in range(hidden_size):
        hidden_gradient = f32(0.0)
        if pre_activation[hidden_index] > 0.0:
            for output_index, gradient in enumerate(output_gradient):
                hidden_gradient = f32(
                    hidden_gradient
                    + f32(gradient * second_weights[hidden_index * output_size + output_index])
                )
        first_bias_gradients[hidden_index] = hidden_gradient
        for input_index in range(input_size):
            first_gradients[input_index * hidden_size + hidden_index] = f32(
                inputs[sample][input_index] * hidden_gradient
            )

    rate = f32(learning_rate)
    first_weights = [f32(value - f32(rate * gradient)) for value, gradient in zip(first_weights, first_gradients)]
    first_bias = [f32(-f32(rate * gradient)) for gradient in first_bias_gradients]
    second_weights = [f32(value - f32(rate * gradient)) for value, gradient in zip(second_weights, second_gradients)]
    second_bias = [f32(-f32(rate * gradient)) for gradient in second_bias_gradients]
    return {
        "first_weights": first_weights,
        "first_bias": first_bias,
        "second_weights": second_weights,
        "second_bias": second_bias,
    }


def run_project(root: Path, log: str = "json") -> tuple[dict, str, str]:
    result = subprocess.run(
        [str(BINARY), "run", str(root), "--format=json", f"--log={log}"],
        cwd=ROOT,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
        timeout=40,
    )
    assert result.returncode == 0, (result.stdout, result.stderr)
    envelope = json.loads(result.stdout)
    return envelope["value"], result.stdout, result.stderr


def main() -> int:
    assert BINARY.is_file(), BINARY
    with tempfile.TemporaryDirectory(prefix="seloryn-mlp-probe-") as directory:
        root = Path(directory)
        continuous = root / "continuous"
        split = root / "split"
        same_seed = root / "same-seed"
        different_seed = root / "different-seed"
        for project in (continuous, split, same_seed):
            create_project(project, 640)
        create_project(different_seed, 640, seed=99)

        continuous_value, _, continuous_log = run_project(continuous)
        same_value, _, same_log = run_project(same_seed)
        different_seed_value, _, _ = run_project(different_seed)
        assert continuous_value == same_value
        assert continuous_value["loss_decreased"] is True
        assert continuous_value["final_loss"] < 1e-3
        assert continuous_value["evaluation_loss"] < 5e-2
        assert continuous_log.splitlines()[0].startswith('{"batch_size":3')
        assert '"operation":"train_mlp"' in continuous_log
        assert continuous_value["checkpoint_sha256"] != different_seed_value["checkpoint_sha256"]

        oracle = root / "oracle"
        create_project(oracle, 1)
        oracle_inputs = [[1.0, -2.0], [-0.5, 2.0]]
        oracle_targets = [[0.75], [-1.25]]
        oracle_data = {"inputs": oracle_inputs, "targets": oracle_targets}
        (oracle / "data" / "train.json").write_text(
            json.dumps(oracle_data) + "\n", encoding="utf-8"
        )
        (oracle / "data" / "test.json").write_text(
            json.dumps(oracle_data) + "\n", encoding="utf-8"
        )
        write_source(oracle, 1, seed=23, batch_size=1, hidden_size=3, learning_rate=0.01)
        run_project(oracle)
        oracle_checkpoint = json.loads(
            (oracle / "checkpoints" / "model.json").read_text(encoding="utf-8")
        )
        expected_weights = oracle_one_step(oracle_inputs, oracle_targets, 23, 3, 0.01)
        for name, expected in expected_weights.items():
            observed = oracle_checkpoint["weights"][name]
            assert len(observed) == len(expected), name
            for actual, reference in zip(observed, expected):
                assert abs(actual - reference) <= 2.0e-6, (name, actual, reference)

        zero_state = root / "zero-state"
        create_project(zero_state, 640, seed=-7046029254386353131)
        zero_state_value, _, _ = run_project(zero_state)
        assert zero_state_value["final_loss"] < 1e-3
        assert zero_state_value["evaluation_loss"] < 5e-2

        budget = root / "budget"
        create_project(budget, 1)
        budget_data = {
            "inputs": TRAIN_DATA["inputs"] + [[4, -2], [4, 0], [4, 2]],
            "targets": TRAIN_DATA["targets"] + [[-3.5], [4.5], [8.5]],
        }
        (budget / "data" / "train.json").write_text(
            json.dumps(budget_data) + "\n", encoding="utf-8"
        )
        write_source(budget, 1_000_000, batch_size=11)
        budget_result = subprocess.run(
            [str(BINARY), "run", str(budget), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert budget_result.returncode != 0
        assert "sample updates exceed maximum" in budget_result.stderr

        work_budget = root / "work-budget"
        create_project(work_budget, 1)
        (work_budget / "data" / "train.json").write_text(
            json.dumps(budget_data) + "\n", encoding="utf-8"
        )
        write_source(work_budget, 100_000, batch_size=1, hidden_size=256)
        work_budget_result = subprocess.run(
            [str(BINARY), "run", str(work_budget), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert work_budget_result.returncode != 0
        assert "work units exceed maximum" in work_budget_result.stderr

        write_source(split, 320)
        run_project(split)
        write_source(split, 640)
        resumed_value, _, resumed_log = run_project(split)
        for field in ("final_loss", "evaluation_loss", "checkpoint_sha256", "steps", "parameter_count"):
            assert resumed_value[field] == continuous_value[field], field
        assert '"resumed_from":320' in resumed_log

        human = root / "human"
        create_project(human, 40)
        _, _, human_log = run_project(human, log="human")
        assert "training.start schema=1 operation=train_mlp" in human_log
        assert "training.step schema=1 operation=train_mlp" in human_log
        assert "training.complete schema=1 operation=train_mlp" in human_log

        checkpoint_path = split / "checkpoints" / "model.json"
        checkpoint_payload = json.loads(checkpoint_path.read_text(encoding="utf-8"))
        original_checkpoint = json.loads(json.dumps(checkpoint_payload))
        assert checkpoint_payload["schema"] == "seloryn.training.checkpoint.v3", checkpoint_payload
        assert checkpoint_payload["runtime"] == "0.12.0", checkpoint_payload
        assert checkpoint_payload["architecture"] == "dense_relu_dense_mse", checkpoint_payload
        assert checkpoint_payload["device"] == "cpu", checkpoint_payload
        assert checkpoint_payload["dtype"] == "F32", checkpoint_payload
        assert checkpoint_payload["shapes"] == {
            "inputs": [2],
            "first_weights": [2, 8],
            "first_bias": [8],
            "second_weights": [8, 1],
            "second_bias": [1],
            "outputs": [1],
        }, checkpoint_payload
        assert checkpoint_payload["features"] == {
            "kind": "ordered_numeric_matrix",
            "order": "source-column-order",
            "count": 2,
        }, checkpoint_payload
        assert checkpoint_payload["transform"] == {"name": "identity", "version": 1}, checkpoint_payload
        assert checkpoint_payload["hash"]["algorithm"] == "sha256", checkpoint_payload
        assert checkpoint_payload["hash"]["weights"] == checkpoint_payload["weights_sha256"], checkpoint_payload
        assert checkpoint_payload["hash"]["data"] == checkpoint_payload["data_sha256"], checkpoint_payload
        assert checkpoint_payload["hash"]["state"] == checkpoint_payload["state_sha256"], checkpoint_payload
        assert checkpoint_payload["integrity"] == {
            "algorithm": "sha256",
            "scope": "payload-sections",
            "authenticity": "none",
        }, checkpoint_payload

        def reject_contract_mutation(mutate, expected):
            mutated = json.loads(json.dumps(original_checkpoint))
            mutate(mutated)
            checkpoint_path.write_text(json.dumps(mutated), encoding="utf-8")
            rejected = subprocess.run(
                [str(BINARY), "run", str(split), "--log=json"],
                cwd=ROOT,
                text=True,
                encoding="utf-8",
                capture_output=True,
                check=False,
                timeout=40,
            )
            assert rejected.returncode == 65, (rejected.stdout, rejected.stderr)
            assert expected in rejected.stderr, (expected, rejected.stderr)

        reject_contract_mutation(
            lambda value: value.update({"schema": "seloryn.training.checkpoint.v2"}),
            "schema seloryn.training.checkpoint.v2 is incompatible",
        )
        reject_contract_mutation(
            lambda value: value.update({"runtime": "0.0.0"}),
            "runtime version is incompatible",
        )
        reject_contract_mutation(
            lambda value: value.update({"architecture": "other_mlp"}),
            "architecture is unsupported",
        )
        reject_contract_mutation(
            lambda value: value["shapes"].update({"first_weights": [8, 2]}),
            "shape contract",
        )
        reject_contract_mutation(
            lambda value: value["features"].update({"order": "column-major"}),
            "feature contract",
        )
        reject_contract_mutation(
            lambda value: value["transform"].update({"name": "standardize"}),
            "transform contract",
        )
        reject_contract_mutation(
            lambda value: value["hash"].update({"weights": "0" * 64}),
            "hash metadata",
        )
        reject_contract_mutation(
            lambda value: value["integrity"].update({"authenticity": "signed"}),
            "integrity contract",
        )
        reject_contract_mutation(
            lambda value: value.update({"unexpected": True}),
            "unsupported field `unexpected`",
        )

        checkpoint_path.write_text(json.dumps(original_checkpoint), encoding="utf-8")
        checkpoint_payload["weights"]["first_bias"][0] += 1.0
        checkpoint_path.write_text(json.dumps(checkpoint_payload), encoding="utf-8")
        tampered = subprocess.run(
            [str(BINARY), "run", str(split), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert tampered.returncode != 0
        assert "weights hash does not match" in tampered.stderr

        checkpoint_payload = json.loads(json.dumps(original_checkpoint))
        checkpoint_payload["data"]["cursor"] += 1
        checkpoint_path.write_text(json.dumps(checkpoint_payload), encoding="utf-8")
        tampered_data = subprocess.run(
            [str(BINARY), "run", str(split), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert tampered_data.returncode != 0
        assert "training state hash does not match" in tampered_data.stderr

        checkpoint_path.write_text(json.dumps(original_checkpoint), encoding="utf-8")
        checkpoint_payload = json.loads(json.dumps(original_checkpoint))
        checkpoint_payload["step"] = 0
        checkpoint_path.write_text(json.dumps(checkpoint_payload), encoding="utf-8")
        tampered_step = subprocess.run(
            [str(BINARY), "run", str(split), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert tampered_step.returncode != 0
        assert "training state hash does not match" in tampered_step.stderr

        checkpoint_payload = json.loads(checkpoint_path.read_text(encoding="utf-8"))
        checkpoint_payload["optimizer"]["name"] = "adam"
        checkpoint_path.write_text(json.dumps(checkpoint_payload), encoding="utf-8")
        unsupported_optimizer = subprocess.run(
            [str(BINARY), "run", str(split), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert unsupported_optimizer.returncode != 0
        assert "optimizer is unsupported" in unsupported_optimizer.stderr

        checkpoint_path.write_text('{"schema":"broken"}\n', encoding="utf-8")
        broken = subprocess.run(
            [str(BINARY), "run", str(split), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=40,
        )
        assert broken.returncode != 0
        assert "checkpoint schema is unsupported" in broken.stderr

        streaming = root / "streaming"
        create_project(streaming, 20000)
        process = subprocess.Popen(
            [str(BINARY), "run", str(streaming), "--log=json"],
            cwd=ROOT,
            text=True,
            encoding="utf-8",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        first_event = process.stderr.readline()
        assert '"event":"training.start"' in first_event
        assert process.poll() is None, "training.start was not observed while the process was alive"
        stdout, stderr = process.communicate(timeout=40)
        assert process.returncode == 0, (stdout, stderr)
        events = [json.loads(first_event)] + [json.loads(line) for line in stderr.splitlines()]
        assert events[0]["event"] == "training.start"
        assert events[-1]["event"] == "training.complete"
        steps = [event["step"] for event in events if event["event"] == "training.step"]
        assert steps == sorted(set(steps))
        assert steps[-1] == 20000

    print("Seloryn MLP training verification: loss, seed, resume, corruption and live stream passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
