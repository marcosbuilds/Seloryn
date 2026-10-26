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


CONTEXT_SIZE = 4


def windows(tokens: list[str], token_id: dict[str, int]) -> list[tuple[list[int], int]]:
    ids = [token_id[token] for token in tokens]
    return [(ids[index : index + CONTEXT_SIZE], ids[index + CONTEXT_SIZE]) for index in range(len(ids) - CONTEXT_SIZE)]


def logits(weights: list[list[float]], context: list[int], vocabulary_size: int) -> list[float]:
    result = [0.0 for _ in range(vocabulary_size)]
    for position, input_id in enumerate(context):
        for output_id, value in enumerate(weights[position * vocabulary_size + input_id]):
            result[output_id] += value
    return result


def train(weights: list[list[float]], data: list[tuple[list[int], int]], steps: int, vocabulary_size: int) -> None:
    scale = float(len(data))
    for _ in range(steps):
        gradients = [[0.0 for _ in row] for row in weights]
        for context, target_id in data:
            probabilities = softmax(logits(weights, context, vocabulary_size))
            for input_position, input_id in enumerate(context):
                row = gradients[input_position * vocabulary_size + input_id]
                for output_id, probability in enumerate(probabilities):
                    row[output_id] += probability / scale
                row[target_id] -= 1.0 / scale
        for input_id in range(len(weights)):
            for output_id in range(len(weights[input_id])):
                weights[input_id][output_id] -= 0.5 * gradients[input_id][output_id]


def loss(weights: list[list[float]], data: list[tuple[list[int], int]], vocabulary_size: int) -> float:
    return sum(-math.log(softmax(logits(weights, context, vocabulary_size))[target_id]) for context, target_id in data) / len(data)


def reference(repo_root: Path) -> dict:
    project = repo_root / "examples" / "projects" / "self-hosted-language-model"
    vocabulary = json.loads((project / "data" / "vocab.json").read_text(encoding="utf-8"))["tokens"]
    token_id = {token: index for index, token in enumerate(vocabulary)}
    train_tokens = json.loads((project / "data" / "train.json").read_text(encoding="utf-8"))["tokens"]
    test_tokens = json.loads((project / "data" / "test.json").read_text(encoding="utf-8"))["tokens"]
    train_data = windows(train_tokens, token_id)
    test_data = windows(test_tokens, token_id)
    vocabulary_size = len(vocabulary)
    weights = [[0.0 for _ in vocabulary] for _ in range(CONTEXT_SIZE * vocabulary_size)]
    initial_loss = loss(weights, train_data, vocabulary_size)
    train(weights, train_data, 160, vocabulary_size)
    predictions = [max(range(vocabulary_size), key=lambda output_id: logits(weights, context, vocabulary_size)[output_id]) for context, _ in test_data]
    accuracy = sum(prediction == target_id for prediction, (_, target_id) in zip(predictions, test_data)) / len(test_data)
    generated_ids: list[int] = []
    context = [token_id["<bos>"] for _ in range(CONTEXT_SIZE)]
    for _ in range(4):
        next_id = max(range(vocabulary_size), key=lambda output_id: logits(weights, context, vocabulary_size)[output_id])
        generated_ids.append(next_id)
        if vocabulary[next_id] == "<eos>":
            break
        context = context[1:] + [next_id]
    generated = " ".join(vocabulary[index] for index in generated_ids)
    test_loss = loss(weights, test_data, vocabulary_size)
    return {
        "status": "ok",
        "architecture": "contextual_linear_softmax",
        "context_size": CONTEXT_SIZE,
        "steps": 160,
        "initial_loss": initial_loss,
        "final_loss": loss(weights, train_data, vocabulary_size),
        "test_loss": test_loss,
        "test_perplexity": math.exp(test_loss),
        "test_accuracy": accuracy,
        "generated": generated,
    }
