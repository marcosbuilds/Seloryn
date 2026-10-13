"""Independent, dependency-free CLI evidence for Seloryn N1.

This script is a black-box test: it never imports the Rust tensor engine.
"""

import argparse
import json
import math
import random
import re
import subprocess
import sys
import tempfile
from pathlib import Path

TENSOR = re.compile(r"Tensor\[(F32|I64), shape=(\[[^\]]*\]), values=(\[[^\]]*\])\]")


def literal(values, dtype):
    return '[' + ', '.join(f'{v}.0' if dtype == 'F32' else str(v) for v in values) + ']'


def matmul_ref(left, right, rows, inner, cols):
    return [sum(left[r * inner + k] * right[k * cols + c] for k in range(inner))
            for r in range(rows) for c in range(cols)]


def main_source(body, dtype='Tensor[F32]'):
    return f'fn main() -> {dtype}:\n' + ''.join(f'    {line}\n' for line in body)


def run(binary, command, path):
    return subprocess.run([str(binary), command, str(path)], capture_output=True, text=True,
                          encoding='utf-8', errors='replace', timeout=40, check=False)


def tensor_output(output, dtype, shape, expected):
    match = TENSOR.fullmatch(output.strip())
    assert match, f'not a tensor result: {output!r}'
    actual_dtype, actual_shape, actual_values = match.groups()
    assert actual_dtype == dtype, (actual_dtype, dtype)
    assert json.loads(actual_shape) == shape, (actual_shape, shape)
    values = json.loads(actual_values)
    assert len(values) == len(expected), (values, expected)
    for value, wanted in zip(values, expected):
        if dtype == 'I64':
            assert value == wanted, (value, wanted)
        else:
            assert math.isclose(value, wanted, rel_tol=1e-5, abs_tol=1e-5), (value, wanted)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--bin', type=Path)
    parser.add_argument('--self-check', action='store_true')
    args = parser.parse_args()
    assert matmul_ref([1, 2, 3, 4], [5, 6, 7, 8], 2, 2, 2) == [19, 22, 43, 50]
    if args.self_check:
        print('Independent numerical oracle: OK (no Seloryn executable was run)')
        return
    default = Path('target/release/seloryn.exe' if sys.platform == 'win32' else 'target/release/seloryn')
    binary = (args.bin or default).resolve()
    assert binary.is_file(), f'No executable at {binary}'
    cases = []
    rng = random.Random(20261012)
    for index in range(24):
        rows, inner, cols = (rng.randrange(1, 5) for _ in range(3))
        left = [rng.randrange(-5, 6) for _ in range(rows * inner)]
        right = [rng.randrange(-5, 6) for _ in range(inner * cols)]
        body = [
            f'let a: Tensor[F32] = tensor_f32({literal(left, "F32")}, [{rows}, {inner}])',
            f'let b: Tensor[F32] = tensor_f32({literal(right, "F32")}, [{inner}, {cols}])',
            'return matmul(a, b)',
        ]
        cases.append((f'matmul-{index}', main_source(body), 'F32', [rows, cols],
                      matmul_ref(left, right, rows, inner, cols)))
    for index in range(12):
        left = [rng.randrange(-9, 10) for _ in range(2)]
        right = [rng.randrange(-9, 10) for _ in range(3)]
        body = [
            f'let a: Tensor[F32] = tensor_f32({literal(left, "F32")}, [2, 1])',
            f'let b: Tensor[F32] = tensor_f32({literal(right, "F32")}, [1, 3])',
            'return a + b',
        ]
        cases.append((f'broadcast-{index}', main_source(body), 'F32', [2, 3],
                      [a + b for a in left for b in right]))
    cases.extend([
        ('linear-forward', main_source([
            'let input: Tensor[F32] = tensor_f32([1.0, 2.0, 3.0], [1, 3])',
            'let weights: Tensor[F32] = tensor_f32([2.0, 0.0, 0.0, 1.0, 1.0, 1.0], [3, 2])',
            'let bias: Tensor[F32] = tensor_f32([1.0, -1.0], [1, 2])',
            'return matmul(input, weights) + bias',
        ]), 'F32', [1, 2], [6.0, 4.0]),
        ('i64-reshape', main_source([
            'let t: Tensor[I64] = tensor_i64([1, 2, 3, 4], [2, 2])',
            'return reshape(t, [4])',
        ], 'Tensor[I64]'), 'I64', [4], [1, 2, 3, 4]),
        ('empty', main_source(['return tensor_zeros_f32([0, 3])']), 'F32', [0, 3], []),
        ('empty-sum', main_source(['return tensor_sum(tensor_zeros_f32([0, 3]))']), 'F32', [], [0.0]),
        ('mean', main_source([
            'return tensor_mean(tensor_f32([1.0, 2.0, 3.0, 4.0], [2, 2]))'
        ]), 'F32', [], [2.5]),
    ])
    pure = [
        ('factorial',
         'fn factorial(value: I64) -> I64:\n'
         '    if value <= 1:\n        return 1\n'
         '    else:\n        return value * factorial(value - 1)\n'
         'fn main() -> I64:\n    return factorial(5)\n', '120\n'),
        ('option', main_source([
            'let maybe: Option[I64] = None',
            'return maybe ?? 7',
        ], 'I64'), '7\n'),
        ('pipeline',
         'fn inc(value: I64) -> I64:\n    return value + 1\n'
         'fn main() -> I64:\n    return 41 |> inc\n', '42\n'),
        ('enum-match',
         'enum Signal:\n    Ready\n    Value(I64)\n'
         'fn read(signal: Signal) -> I64:\n'
         '    match signal:\n'
         '        case Signal.Value(number):\n            return number\n'
         '        case Signal.Ready:\n            return 0\n'
         'fn main() -> I64:\n    return read(Signal.Value(4))\n', '4\n'),
    ]
    negative = [
        ('shape', main_source(['return tensor_f32([1.0, 2.0, 3.0], [2, 2])']), 'SEL-R203'),
        ('broadcast', main_source([
            'return tensor_zeros_f32([2, 3]) + tensor_zeros_f32([2, 4])'
        ]), 'SEL-R204'),
        ('matmul-shape', main_source([
            'return matmul(tensor_zeros_f32([2, 3]), tensor_zeros_f32([4, 2]))'
        ]), 'SEL-R205'),
        ('budget', main_source(['return tensor_zeros_f32([5000, 5000])']), 'SEL-R202'),
        ('global-budget', main_source([
            'let first: Tensor[F32] = tensor_zeros_f32([6291456])',
            'let second: Tensor[F32] = tensor_zeros_f32([6291456])',
            'return tensor_zeros_f32([4194305])',
        ]), 'SEL-R209'),
        ('mean-empty', main_source(['return tensor_mean(tensor_zeros_f32([0, 3]))']), 'SEL-R207'),
        ('div-zero', main_source([
            'return tensor_f32([1.0], [1]) / tensor_zeros_f32([1])'
        ]), 'SEL-R206'),
        ('overflow', main_source([
            'return tensor_i64([9223372036854775807], [1]) + tensor_i64([1], [1])'
        ], 'Tensor[I64]'), 'SEL-R206'),
        ('static-type', main_source([
            'return tensor_i64([1], [1]) + tensor_f32([1.0], [1])'
        ]), 'SEL-T003'),
        ('result-error',
         'fn validate() -> Result[I64, Text]:\n'
         '    require false else "bad"\n    return Ok(1)\n'
         'fn main() -> Result[I64, Text]:\n    return validate()\n',
         'SEL-R127'),
    ]
    count = 0
    with tempfile.TemporaryDirectory(prefix='seloryn-n1-proof-') as tmp:
        directory = Path(tmp)
        for name, source, dtype, shape, expected in cases:
            path = directory / f'{name}.sely'
            path.write_text(source, encoding='utf-8', newline='\n')
            checked = run(binary, 'check', path)
            assert checked.returncode == 0, f'{name} static check: {checked.stderr}'
            actual = run(binary, 'run', path)
            assert actual.returncode == 0, f'{name} runtime: {actual.stderr}'
            tensor_output(actual.stdout, dtype, shape, expected)
            count += 1
        for name, source, expected in pure:
            path = directory / f'{name}.sely'
            path.write_text(source, encoding='utf-8', newline='\n')
            checked = run(binary, 'check', path)
            assert checked.returncode == 0, f'{name} check: {checked.stderr}'
            actual = run(binary, 'run', path)
            assert actual.returncode == 0, f'{name} runtime: {actual.stderr}'
            assert actual.stdout == expected, (name, actual.stdout, expected)
            count += 1
        for name, source, code in negative:
            path = directory / f'{name}.sely'
            path.write_text(source, encoding='utf-8', newline='\n')
            first = run(binary, 'run', path)
            second = run(binary, 'run', path)
            assert first.returncode == 65, (name, first.returncode, first.stderr)
            assert first.stdout == '', (name, first.stdout)
            assert code in first.stderr, (name, code, first.stderr)
            assert (first.returncode, first.stdout, first.stderr) == (second.returncode, second.stdout, second.stderr), name
            count += 1
    print(f'Seloryn N1 black-box verification: {count} cases passed')


if __name__ == '__main__':
    try:
        main()
    except Exception as exc:
        print(f'N1 verification FAILED: {exc!r}', file=sys.stderr)
        raise SystemExit(1) from exc
