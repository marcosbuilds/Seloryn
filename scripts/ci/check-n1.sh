#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo fmt --all -- --check
cargo test --all-targets --locked -- --test-threads=1
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --all-targets --locked -- --test-threads=1
cargo build --release --locked
python3 tests/n1_probe.py
python3 tests/n2_probe.py
python3 tests/n3_probe.py
python3 tests/release_probe.py
./target/release/seloryn run examples/tensors/linear-forward.sely
./target/release/seloryn run examples/tensors/tensor-reduce.sely
printf 'Seloryn public checks completed successfully\n'
