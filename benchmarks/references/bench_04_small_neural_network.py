"""Independent stdlib oracle for BENCH-04 Small Neural Network.

The candidate owns Tensor[F32] parameters and kernels.  The oracle therefore
rounds every primitive operation to IEEE-754 binary32 instead of accidentally
accumulating 160 updates in Python's binary64 floats.
"""

from __future__ import annotations

import json
import struct
from collections.abc import Iterable
from pathlib import Path


def f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def add(left: float, right: float) -> float:
    return f32(left + right)


def subtract(left: float, right: float) -> float:
    return f32(left - right)


def multiply(left: float, right: float) -> float:
    return f32(left * right)


def divide(left: float, right: float) -> float:
    return f32(left / right)


def dot(row: list[float], column: list[float]) -> float:
    result = f32(0.0)
    for left, right in zip(row, column):
        result = add(result, multiply(left, right))
    return result


def sum_f32(values: Iterable[float]) -> float:
    result = f32(0.0)
    for value in values:
        result = add(result, value)
    return result


def forward(inputs: list[list[float]], first: list[list[float]], first_bias: list[float], second: list[list[float]], second_bias: float) -> list[float]:
    predictions = []
    for row in inputs:
        hidden = [max(0.0, add(dot(row, [first[feature][unit] for feature in range(2)]), first_bias[unit])) for unit in range(3)]
        predictions.append(add(dot(hidden, [second[unit][0] for unit in range(3)]), second_bias))
    return predictions


def loss(predictions: list[float], targets: list[float]) -> float:
    total = f32(0.0)
    for prediction, target in zip(predictions, targets):
        error = subtract(prediction, target)
        total = add(total, multiply(error, error))
    return divide(total, f32(len(targets)))


def reference(repo_root: Path) -> dict:
    project = repo_root / "examples" / "projects" / "self-hosted-mlp"
    train = json.loads((project / "data" / "train.json").read_text(encoding="utf-8"))
    test = json.loads((project / "data" / "test.json").read_text(encoding="utf-8"))
    inputs = [[f32(train["inputs"][index]), f32(train["inputs"][index + 1])] for index in range(0, len(train["inputs"]), 2)]
    targets = [f32(value) for value in train["targets"]]
    test_inputs = [[f32(test["inputs"][index]), f32(test["inputs"][index + 1])] for index in range(0, len(test["inputs"]), 2)]
    test_targets = [f32(value) for value in test["targets"]]
    first = [[f32(0.2), f32(-0.1), f32(-0.3)], [f32(0.4), f32(0.1), f32(0.2)]]
    first_bias = [f32(0.1), f32(0.1), f32(0.1)]
    second = [[f32(0.5)], [f32(-0.2)], [f32(0.3)]]
    second_bias = f32(0.1)
    initial_loss = loss(forward(inputs, first, first_bias, second, second_bias), targets)
    rate = f32(0.05)
    mean_scale = f32(1.0 / len(targets))
    for _ in range(160):
        hidden_rows = []
        preactivation_rows = []
        predictions = []
        for row in inputs:
            preactivation = [add(dot(row, [first[feature][unit] for feature in range(2)]), first_bias[unit]) for unit in range(3)]
            hidden = [max(0.0, value) for value in preactivation]
            hidden_rows.append(hidden)
            preactivation_rows.append(preactivation)
            predictions.append(add(dot(hidden, [second[unit][0] for unit in range(3)]), second_bias))
        output_gradient = [
            add(multiply(mean_scale, subtract(prediction, target)), multiply(mean_scale, subtract(prediction, target)))
            for prediction, target in zip(predictions, targets)
        ]
        second_gradient = []
        for unit in range(3):
            gradient = f32(0.0)
            for row in range(len(inputs)):
                gradient = add(gradient, multiply(hidden_rows[row][unit], output_gradient[row]))
            second_gradient.append([gradient])
        second_bias_gradient = f32(0.0)
        for gradient in output_gradient:
            second_bias_gradient = add(second_bias_gradient, gradient)
        hidden_gradient = [[multiply(output_gradient[row], second[unit][0]) for unit in range(3)] for row in range(len(inputs))]
        first_gradient = [[
            sum_f32(
                multiply(
                    multiply(inputs[row][feature], hidden_gradient[row][unit]),
                    f32(1.0 if preactivation_rows[row][unit] > 0.0 else 0.0),
                )
                for row in range(len(inputs))
            )
            for unit in range(3)
        ] for feature in range(2)]
        first_bias_gradient = [
            sum_f32(
                multiply(hidden_gradient[row][unit], f32(1.0 if preactivation_rows[row][unit] > 0.0 else 0.0))
                for row in range(len(inputs))
            )
            for unit in range(3)
        ]
        for feature in range(2):
            for unit in range(3):
                first[feature][unit] = subtract(first[feature][unit], multiply(rate, first_gradient[feature][unit]))
        for unit in range(3):
            first_bias[unit] = subtract(first_bias[unit], multiply(rate, first_bias_gradient[unit]))
            second[unit][0] = subtract(second[unit][0], multiply(rate, second_gradient[unit][0]))
        second_bias = subtract(second_bias, multiply(rate, second_bias_gradient))
    final_loss = loss(forward(inputs, first, first_bias, second, second_bias), targets)
    evaluation_loss = loss(forward(test_inputs, first, first_bias, second, second_bias), test_targets)
    return {
        "status": "ok",
        "architecture": "custom_relu_mlp",
        "steps": 160,
        "initial_loss": initial_loss,
        "final_loss": final_loss,
        "evaluation_loss": evaluation_loss,
        "reloaded_loss": evaluation_loss,
        "prediction": forward(test_inputs, first, first_bias, second, second_bias)[0],
    }
