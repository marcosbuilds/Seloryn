"""Independent stdlib oracle for BENCH-04 Small Neural Network."""

from __future__ import annotations

import json
from pathlib import Path


def dot(row: list[float], column: list[float]) -> float:
    return sum(left * right for left, right in zip(row, column))


def forward(inputs: list[list[float]], first: list[list[float]], first_bias: list[float], second: list[list[float]], second_bias: float) -> list[float]:
    predictions = []
    for row in inputs:
        hidden = [max(0.0, dot(row, [first[feature][unit] for feature in range(2)]) + first_bias[unit]) for unit in range(3)]
        predictions.append(dot(hidden, [second[unit][0] for unit in range(3)]) + second_bias)
    return predictions


def loss(predictions: list[float], targets: list[float]) -> float:
    return sum((prediction - target) ** 2 for prediction, target in zip(predictions, targets)) / len(targets)


def reference(repo_root: Path) -> dict:
    project = repo_root / "examples" / "projects" / "self-hosted-mlp"
    train = json.loads((project / "data" / "train.json").read_text(encoding="utf-8"))
    test = json.loads((project / "data" / "test.json").read_text(encoding="utf-8"))
    inputs = [[float(train["inputs"][index]), float(train["inputs"][index + 1])] for index in range(0, len(train["inputs"]), 2)]
    targets = [float(value) for value in train["targets"]]
    test_inputs = [[float(test["inputs"][index]), float(test["inputs"][index + 1])] for index in range(0, len(test["inputs"]), 2)]
    test_targets = [float(value) for value in test["targets"]]
    first = [[0.2, -0.1, -0.3], [0.4, 0.1, 0.2]]
    first_bias = [0.1, 0.1, 0.1]
    second = [[0.5], [-0.2], [0.3]]
    second_bias = 0.1
    initial_loss = loss(forward(inputs, first, first_bias, second, second_bias), targets)
    rate = 0.05
    for _ in range(160):
        hidden_rows = []
        preactivation_rows = []
        predictions = []
        for row in inputs:
            preactivation = [dot(row, [first[feature][unit] for feature in range(2)]) + first_bias[unit] for unit in range(3)]
            hidden = [max(0.0, value) for value in preactivation]
            hidden_rows.append(hidden)
            preactivation_rows.append(preactivation)
            predictions.append(dot(hidden, [second[unit][0] for unit in range(3)]) + second_bias)
        output_gradient = [2.0 * (prediction - target) / len(targets) for prediction, target in zip(predictions, targets)]
        second_gradient = [[sum(hidden_rows[row][unit] * output_gradient[row] for row in range(len(inputs)))] for unit in range(3)]
        second_bias_gradient = sum(output_gradient)
        hidden_gradient = [[output_gradient[row] * second[unit][0] for unit in range(3)] for row in range(len(inputs))]
        first_gradient = [[
            sum(inputs[row][feature] * hidden_gradient[row][unit] * (1.0 if preactivation_rows[row][unit] > 0.0 else 0.0) for row in range(len(inputs)))
            for unit in range(3)
        ] for feature in range(2)]
        first_bias_gradient = [sum(hidden_gradient[row][unit] * (1.0 if preactivation_rows[row][unit] > 0.0 else 0.0) for row in range(len(inputs))) for unit in range(3)]
        for feature in range(2):
            for unit in range(3):
                first[feature][unit] -= rate * first_gradient[feature][unit]
        for unit in range(3):
            first_bias[unit] -= rate * first_bias_gradient[unit]
            second[unit][0] -= rate * second_gradient[unit][0]
        second_bias -= rate * second_bias_gradient
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
