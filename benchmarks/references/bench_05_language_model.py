"""Independent stdlib oracle for BENCH-05 Language Model."""

from __future__ import annotations

import json
import math
from pathlib import Path


def softmax(row: list[float]) -> list[float]:
    maximum = max(row)
    values = [math.exp(value - maximum) for value in row]
    total = sum(values)
    return [value / total for value in values]


def pairs(tokens: list[str], token_id: dict[str, int]) -> list[tuple[int, int]]:
    return [(token_id[tokens[index]], token_id[tokens[index + 1]]) for index in range(len(tokens) - 1)]


def train(weights: list[list[float]], data: list[tuple[int, int]], steps: int) -> None:
    scale = float(len(data))
    for _ in range(steps):
        gradients = [[0.0 for _ in row] for row in weights]
        for input_id, target_id in data:
            probabilities = softmax(weights[input_id])
            for output_id, probability in enumerate(probabilities):
                gradients[input_id][output_id] += probability / scale
            gradients[input_id][target_id] -= 1.0 / scale
        for input_id in range(len(weights)):
            for output_id in range(len(weights[input_id])):
                weights[input_id][output_id] -= 0.5 * gradients[input_id][output_id]


def loss(weights: list[list[float]], data: list[tuple[int, int]]) -> float:
    return sum(-math.log(softmax(weights[input_id])[target_id]) for input_id, target_id in data) / len(data)


def reference(repo_root: Path) -> dict:
    project = repo_root / "examples" / "projects" / "self-hosted-language-model"
    vocabulary = json.loads((project / "data" / "vocab.json").read_text(encoding="utf-8"))["tokens"]
    token_id = {token: index for index, token in enumerate(vocabulary)}
    train_tokens = json.loads((project / "data" / "train.json").read_text(encoding="utf-8"))["tokens"]
    test_tokens = json.loads((project / "data" / "test.json").read_text(encoding="utf-8"))["tokens"]
    train_data = pairs(train_tokens, token_id)
    test_data = pairs(test_tokens, token_id)
    weights = [[0.0 for _ in vocabulary] for _ in vocabulary]
    initial_loss = loss(weights, train_data)
    train(weights, train_data, 160)
    predictions = [max(range(len(vocabulary)), key=lambda output_id: weights[input_id][output_id]) for input_id, _ in test_data]
    accuracy = sum(prediction == target_id for prediction, (_, target_id) in zip(predictions, test_data)) / len(test_data)
    generated_ids = [token_id["<bos>"], token_id["the"]]
    for _ in range(4):
        next_id = max(range(len(vocabulary)), key=lambda output_id: weights[generated_ids[-1]][output_id])
        generated_ids.append(next_id)
        if vocabulary[next_id] == "<eos>":
            break
    generated = " ".join(vocabulary[index] for index in generated_ids[1:])
    return {
        "status": "ok",
        "architecture": "bigram_softmax",
        "steps": 160,
        "initial_loss": initial_loss,
        "final_loss": loss(weights, train_data),
        "test_loss": loss(weights, test_data),
        "test_accuracy": accuracy,
        "generated": generated,
    }
