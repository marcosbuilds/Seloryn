#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo fmt --all -- --check
cargo test --all-targets --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --all-targets --locked
cargo build --release --locked
python3 tests/n1_probe.py
./target/release/seloryn run examples/tensors/linear-forward.sely
./target/release/seloryn run examples/tensors/tensor-reduce.sely
printf 'Seloryn N1 checks completed successfully\n'
