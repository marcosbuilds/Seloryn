"""Independent black-box proof for the bounded CPU MLP trainer."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target" / "release" / ("seloryn.exe" if sys.platform == "win32" else "seloryn")


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
        '{"inputs":[[0,0],[0,1],[1,0],[1,1]],"targets":[[0],[1],[1],[0]]}\n',
        encoding="utf-8",
    )
    (root / "data" / "test.json").write_text(
        '{"inputs":[[0,0],[0,1],[1,0],[1,1]],"targets":[[0],[1],[1],[0]]}\n',
        encoding="utf-8",
    )
    write_source(root, steps, seed)


def write_source(root: Path, steps: int, seed: int = 17) -> None:
    (root / "src" / "main.sely").write_text(
        f'from training import train_mlp\n\nfn main() -> Json:\n'
        f'    return train_mlp("data/train.json", "data/test.json", '
        f'"checkpoints/model.json", learning_rate=0.1, hidden_size=8, '
        f'batch_size=3, steps={steps}, seed={seed})\n',
        encoding="utf-8",
    )


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
            create_project(project, 160)
        create_project(different_seed, 160, seed=99)

        continuous_value, _, continuous_log = run_project(continuous)
        same_value, _, same_log = run_project(same_seed)
        different_seed_value, _, _ = run_project(different_seed)
        assert continuous_value == same_value
        assert continuous_value["loss_decreased"] is True
        assert continuous_value["final_loss"] < 1e-3
        assert continuous_value["evaluation_loss"] < 1e-3
        assert continuous_log.splitlines()[0].startswith('{"batch_size":3')
        assert '"operation":"train_mlp"' in continuous_log
        assert continuous_value["checkpoint_sha256"] != different_seed_value["checkpoint_sha256"]

        write_source(split, 80)
        run_project(split)
        write_source(split, 160)
        resumed_value, _, resumed_log = run_project(split)
        for field in ("final_loss", "evaluation_loss", "checkpoint_sha256", "steps", "parameter_count"):
            assert resumed_value[field] == continuous_value[field], field
        assert '"resumed_from":80' in resumed_log

        human = root / "human"
        create_project(human, 20)
        _, _, human_log = run_project(human, log="human")
        assert "training.start schema=1 operation=train_mlp" in human_log
        assert "training.step schema=1 operation=train_mlp" in human_log
        assert "training.complete schema=1 operation=train_mlp" in human_log

        checkpoint_path = split / "checkpoints" / "model.json"
        checkpoint_payload = json.loads(checkpoint_path.read_text(encoding="utf-8"))
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
        assert '"event":"training.complete"' in stderr

    print("Seloryn MLP training verification: loss, seed, resume, corruption and live stream passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
