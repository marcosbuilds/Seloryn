#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
python3 scripts/ci/public_boundary.py
python3 scripts/ci/check-docs.py
python3 -m pip install --disable-pip-version-check --quiet -r scripts/release/requirements.txt
python3 scripts/release/verify-version.py
python3 tests/versioning_probe.py
cargo fmt --all -- --check
cargo test --all-targets --locked -- --test-threads=1
cargo clippy --all-targets --locked -- -D warnings
cargo test --release --all-targets --locked -- --test-threads=1
cargo build --release --locked
python3 scripts/ci/conformance.py --run --suite core
python3 scripts/ci/conformance.py --run --suite benchmark
python3 scripts/ci/conformance.py --run --suite hardware
python3 scripts/ci/conformance.py --run --suite release
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
