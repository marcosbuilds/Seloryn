#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 -m pip install --disable-pip-version-check --quiet -r scripts/release/requirements.txt
python3 scripts/release/verify-version.py
python3 tests/versioning_probe.py
cargo fmt --all -- --check
cargo test --all-targets --locked -- --test-threads=1
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --all-targets --locked -- --test-threads=1
cargo build --release --locked
python3 tests/n1_probe.py
python3 tests/n2_probe.py
python3 tests/n3_probe.py
python3 tests/n3_tensor_probe.py
python3 tests/release_probe.py
python3 tests/release_staging_probe.py
python3 tests/lsp_probe.py
python3 tests/project_manifest_probe.py
python3 tests/json_array_probe.py
python3 tests/mlp_training_probe.py
python3 tests/streaming_v2_probe.py
python3 tests/self_hosted_probe.py
python3 tests/gpu_probe.py
python3 tests/gpu_training_probe.py
./target/release/seloryn run examples/tensors/linear-forward.sely
./target/release/seloryn run examples/tensors/tensor-reduce.sely
set +e
./target/release/seloryn run tests/result-error.sely
result_error_status=$?
set -e
if [ "$result_error_status" -ne 65 ]; then
    printf 'Expected result-error fixture to exit 65, got %s\n' "$result_error_status" >&2
    exit 1
fi
printf 'Seloryn public checks completed successfully\n'
