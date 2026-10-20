"""Independent pure-Python reference for the auditable Seloryn fixture.

This intentionally uses only the standard library. It mirrors the mathematical
contract, not Seloryn's implementation, so the probe can compare results
without importing the runtime or a training helper.
"""

from __future__ import annotations

import json
from pathlib import Path


PROJECT = Path(__file__).resolve().parents[1]


def _load(name: str) -> dict:
    return json.loads((PROJECT / "data" / name).read_text(encoding="utf-8"))


def _rows(document: dict) -> tuple[list[list[float]], list[float]]:
    values = document["inputs"]
    return [values[index : index + 4] for index in range(0, len(values), 4)], document["targets"]


def _forward(
    inputs: list[list[float]],
    first: list[list[float]],
    first_bias: list[float],
    second: list[float],
    second_bias: float,
) -> list[float]:
    outputs: list[float] = []
    for row in inputs:
        hidden = [
            max(0.0, sum(row[index] * first[index][unit] for index in range(4)) + first_bias[unit])
            for unit in range(4)
        ]
        outputs.append(sum(hidden[unit] * second[unit] for unit in range(4)) + second_bias)
    return outputs


def _loss(predictions: list[float], targets: list[float]) -> float:
    return sum((prediction - target) ** 2 for prediction, target in zip(predictions, targets)) / len(targets)


def train_reference(steps: int = 320, learning_rate: float = 0.05) -> dict[str, float]:
    train_inputs, train_targets = _rows(_load("train.json"))
    test_inputs, test_targets = _rows(_load("test.json"))
    first = [
        [0.2, -0.1, 0.1, 0.3],
        [-0.3, 0.4, 0.1, 0.2],
        [0.2, -0.2, 0.3, 0.1],
        [0.4, -0.1, 0.2, 0.3],
    ]
    first_bias = [0.1, 0.1, 0.1, 0.1]
    second = [0.5, -0.2, 0.3, 0.4]
    second_bias = 0.1
    initial_loss = _loss(_forward(train_inputs, first, first_bias, second, second_bias), train_targets)

    for _ in range(steps):
        first_gradient = [[0.0 for _ in range(4)] for _ in range(4)]
        first_bias_gradient = [0.0 for _ in range(4)]
        second_gradient = [0.0 for _ in range(4)]
        second_bias_gradient = 0.0
        scale = float(len(train_targets))
        for row, target in zip(train_inputs, train_targets):
            pre_activation = [
                sum(row[index] * first[index][unit] for index in range(4)) + first_bias[unit]
                for unit in range(4)
            ]
            hidden = [max(0.0, value) for value in pre_activation]
            prediction = sum(hidden[unit] * second[unit] for unit in range(4)) + second_bias
            output_gradient = 2.0 * (prediction - target) / scale
            second_bias_gradient += output_gradient
            for unit in range(4):
                second_gradient[unit] += hidden[unit] * output_gradient
                hidden_gradient = output_gradient * second[unit] if pre_activation[unit] > 0.0 else 0.0
                first_bias_gradient[unit] += hidden_gradient
                for index in range(4):
                    first_gradient[index][unit] += row[index] * hidden_gradient
        for index in range(4):
            for unit in range(4):
                first[index][unit] -= learning_rate * first_gradient[index][unit]
        for unit in range(4):
            first_bias[unit] -= learning_rate * first_bias_gradient[unit]
            second[unit] -= learning_rate * second_gradient[unit]
        second_bias -= learning_rate * second_bias_gradient

    train_predictions = _forward(train_inputs, first, first_bias, second, second_bias)
    test_predictions = _forward(test_inputs, first, first_bias, second, second_bias)
    return {
        "initial_loss": initial_loss,
        "final_loss": _loss(train_predictions, train_targets),
        "test_loss": _loss(test_predictions, test_targets),
        "accuracy": sum(
            (prediction >= 0.5) == (target >= 0.5)
            for prediction, target in zip(test_predictions, test_targets)
        )
        / len(test_targets),
    }


if __name__ == "__main__":
    print(json.dumps(train_reference(), sort_keys=True))
