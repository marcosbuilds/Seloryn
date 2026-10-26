"""Independent stdlib reference for the embedding + ReLU MLP fixture."""

from __future__ import annotations

import math
import struct


def f32(value: float) -> float:
    return struct.unpack("<f", struct.pack("<f", value))[0]


def add(left: float, right: float) -> float:
    return f32(left + right)


def sub(left: float, right: float) -> float:
    return f32(left - right)


def mul(left: float, right: float) -> float:
    return f32(left * right)


def uniform(shape: tuple[int, ...], seed: int) -> list[float]:
    mask = (1 << 64) - 1
    state = seed & mask
    if state == 0:
        state = 0x9E3779B97F4A7C15
    values: list[float] = []
    for _ in range(math.prod(shape)):
        state ^= state >> 12
        state &= mask
        state ^= (state << 25) & mask
        state &= mask
        state ^= state >> 27
        state &= mask
        random = (state * 0x2545F4914F6CDD1D) & mask
        unit = random / mask
        values.append(f32((unit * 2.0 - 1.0) * 0.1))
    return values


def matmul(left: list[list[float]], right: list[list[float]]) -> list[list[float]]:
    rows = len(left)
    inner = len(right)
    columns = len(right[0])
    return [
        [
            _dot(left[row], [right[index][column] for index in range(inner)])
            for column in range(columns)
        ]
        for row in range(rows)
    ]


def _dot(left: list[float], right: list[float]) -> float:
    total = 0.0
    for left_value, right_value in zip(left, right):
        total = add(total, mul(left_value, right_value))
    return total


def transpose(matrix: list[list[float]]) -> list[list[float]]:
    return [list(column) for column in zip(*matrix)]


def add_bias(matrix: list[list[float]], bias: list[float]) -> list[list[float]]:
    return [[add(value, bias[column]) for column, value in enumerate(row)] for row in matrix]


def softmax(row: list[float]) -> list[float]:
    maximum = max(row)
    exponentials = [math.exp(float(value) - float(maximum)) for value in row]
    normalizer = sum(exponentials)
    return [f32(value / normalizer) for value in exponentials]


def windows(tokens: list[str], token_id: dict[str, int], context_size: int) -> list[tuple[list[int], int]]:
    ids = [token_id[token] for token in tokens]
    return [(ids[index : index + context_size], ids[index + context_size]) for index in range(len(ids) - context_size)]


def forward(
    data: list[tuple[list[int], int]],
    embedding: list[list[float]],
    first: list[list[float]],
    first_bias: list[float],
    second: list[list[float]],
    second_bias: list[float],
) -> tuple[float, list[list[float]], list[list[float]], list[list[float]], list[list[float]], list[list[float]]]:
    losses: list[float] = []
    all_flattened: list[list[float]] = []
    all_hidden: list[list[float]] = []
    all_pre: list[list[float]] = []
    all_probabilities: list[list[float]] = []
    for context, target in data:
        flattened = [value for token in context for value in embedding[token]]
        pre = add_bias(matmul([flattened], first), first_bias)[0]
        hidden = [max(0.0, value) for value in pre]
        logits = add_bias(matmul([hidden], second), second_bias)[0]
        probabilities = softmax(logits)
        losses.append(-(math.log(float(probabilities[target]))))
        all_flattened.append(flattened)
        all_hidden.append(hidden)
        all_pre.append(pre)
        all_probabilities.append(probabilities)
    return (
        f32(sum(losses) / len(data)),
        all_flattened,
        all_hidden,
        all_pre,
        all_probabilities,
        [[float(target)] for _, target in data],
    )


def train(
    train_data: list[tuple[list[int], int]],
    test_data: list[tuple[list[int], int]],
    vocabulary_size: int,
    context_size: int,
    embedding_dim: int,
    hidden_size: int,
    steps: int,
    seed: int,
    token_text: list[str],
    bos: int,
    eos: int,
) -> dict[str, object]:
    embedding_flat = uniform((vocabulary_size, embedding_dim), seed)
    embedding = [embedding_flat[index * embedding_dim : (index + 1) * embedding_dim] for index in range(vocabulary_size)]
    first_flat = uniform((context_size * embedding_dim, hidden_size), seed + 1)
    first = [first_flat[row * hidden_size : (row + 1) * hidden_size] for row in range(context_size * embedding_dim)]
    first_bias = [0.0 for _ in range(hidden_size)]
    second_flat = uniform((hidden_size, vocabulary_size), seed + 2)
    second = [second_flat[row * vocabulary_size : (row + 1) * vocabulary_size] for row in range(hidden_size)]
    second_bias = [0.0 for _ in range(vocabulary_size)]

    initial = forward(train_data, embedding, first, first_bias, second, second_bias)[0]
    for _ in range(steps):
        loss_value, flattened, hidden, pre, probabilities, _ = forward(
            train_data, embedding, first, first_bias, second, second_bias
        )
        del loss_value
        batch = len(train_data)
        d_second = [[0.0 for _ in range(vocabulary_size)] for _ in range(hidden_size)]
        d_second_bias = [0.0 for _ in range(vocabulary_size)]
        d_first = [[0.0 for _ in range(hidden_size)] for _ in range(context_size * embedding_dim)]
        d_first_bias = [0.0 for _ in range(hidden_size)]
        d_embedding = [[0.0 for _ in range(embedding_dim)] for _ in range(vocabulary_size)]
        for row, (context, target) in enumerate(train_data):
            d_logits = [mul(sub(probability, 1.0 if class_id == target else 0.0), f32(1.0 / batch)) for class_id, probability in enumerate(probabilities[row])]
            for hidden_id in range(hidden_size):
                for class_id in range(vocabulary_size):
                    d_second[hidden_id][class_id] = add(d_second[hidden_id][class_id], mul(hidden[row][hidden_id], d_logits[class_id]))
            for class_id in range(vocabulary_size):
                d_second_bias[class_id] = add(d_second_bias[class_id], d_logits[class_id])
            d_hidden = [
                _dot(d_logits, [second[hidden_id][class_id] for class_id in range(vocabulary_size)])
                for hidden_id in range(hidden_size)
            ]
            d_pre = [gradient if pre[row][hidden_id] > 0.0 else 0.0 for hidden_id, gradient in enumerate(d_hidden)]
            for hidden_id, gradient in enumerate(d_pre):
                d_first_bias[hidden_id] = add(d_first_bias[hidden_id], gradient)
            for input_id in range(context_size * embedding_dim):
                for hidden_id in range(hidden_size):
                    d_first[input_id][hidden_id] = add(d_first[input_id][hidden_id], mul(flattened[row][input_id], d_pre[hidden_id]))
            d_flattened = [
                _dot(d_pre, [first[input_id][hidden_id] for hidden_id in range(hidden_size)])
                for input_id in range(context_size * embedding_dim)
            ]
            for position, token in enumerate(context):
                offset = position * embedding_dim
                for dimension in range(embedding_dim):
                    d_embedding[token][dimension] = add(d_embedding[token][dimension], d_flattened[offset + dimension])
        for row in range(vocabulary_size):
            for dimension in range(embedding_dim):
                embedding[row][dimension] = sub(embedding[row][dimension], mul(0.5, d_embedding[row][dimension]))
        for row in range(context_size * embedding_dim):
            for column in range(hidden_size):
                first[row][column] = sub(first[row][column], mul(0.5, d_first[row][column]))
        for column in range(hidden_size):
            first_bias[column] = sub(first_bias[column], mul(0.5, d_first_bias[column]))
        for row in range(hidden_size):
            for column in range(vocabulary_size):
                second[row][column] = sub(second[row][column], mul(0.5, d_second[row][column]))
        for column in range(vocabulary_size):
            second_bias[column] = sub(second_bias[column], mul(0.5, d_second_bias[column]))

    final = forward(train_data, embedding, first, first_bias, second, second_bias)[0]
    test_loss = forward(test_data, embedding, first, first_bias, second, second_bias)[0]
    test_logits: list[list[float]] = []
    for context, _target in test_data:
        flattened = [value for token in context for value in embedding[token]]
        pre = add_bias(matmul([flattened], first), first_bias)[0]
        hidden = [max(0.0, value) for value in pre]
        test_logits.append(add_bias(matmul([hidden], second), second_bias)[0])
    accuracy = sum(max(range(vocabulary_size), key=test_logits[index].__getitem__) == target for index, (_context, target) in enumerate(test_data)) / len(test_data)

    def generate(tokens: list[str], bos: int, eos: int) -> str:
        context = [bos for _ in range(context_size)]
        result: list[str] = []
        for _ in range(8):
            flattened = [value for token in context for value in embedding[token]]
            pre = add_bias(matmul([flattened], first), first_bias)[0]
            hidden = [max(0.0, value) for value in pre]
            logits = add_bias(matmul([hidden], second), second_bias)[0]
            current = max(range(vocabulary_size), key=logits.__getitem__)
            result.append(tokens[current])
            if current == eos:
                break
            context = context[1:] + [current]
        return " ".join(result)

    return {
        "initial_loss": initial,
        "final_loss": final,
        "test_loss": test_loss,
        "test_perplexity": math.exp(test_loss),
        "test_accuracy": accuracy,
        "generated": generate(token_text, bos, eos),
        "embedding": embedding,
        "first_weights": first,
        "first_bias": first_bias,
        "second_weights": second,
        "second_bias": second_bias,
    }
