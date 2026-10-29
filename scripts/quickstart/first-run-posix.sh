#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'SELORYN-QUICKSTART: ERROR: %s\n' "$1" >&2
    exit "${2:-1}"
}

usage() {
    cat >&2 <<'EOF'
usage: first-run-posix.sh [--skip-build] [--doctor-only]
                          [--cargo-command PATH] [--python-command PATH]

Builds the release CLI, runs the read-only project doctor, checks and runs a
tracked tensor example, then runs the portable core conformance suite.
EOF
    exit 64
}

root=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
skip_build=false
doctor_only=false
cargo_command="${SELORYN_CARGO:-cargo}"
python_override="${SELORYN_PYTHON:-}"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --skip-build) skip_build=true ;;
        --doctor-only) doctor_only=true ;;
        --cargo-command)
            shift
            [ "$#" -gt 0 ] || usage
            cargo_command="$1"
            ;;
        --python-command)
            shift
            [ "$#" -gt 0 ] || usage
            python_override="$1"
            ;;
        --help|-h) usage ;;
        *) usage ;;
    esac
    shift
done

doctor_args=()
doctor_args+=(--cargo-command "$cargo_command")
if [ -n "$python_override" ]; then
    doctor_args+=(--python-command "$python_override")
fi

printf '%s\n' '== Seloryn first-run: prerequisites =='
bash "$root/scripts/quickstart/doctor-posix.sh" "${doctor_args[@]}"
if [ "$doctor_only" = true ]; then
    exit 0
fi

if [ "$skip_build" = false ]; then
    printf '%s\n' '' '> cargo build --release --locked'
    "$cargo_command" build --release --locked
fi

binary="$root/target/release/seloryn"
if [ ! -f "$binary" ] && [ -f "$root/target/release/seloryn.exe" ]; then
    binary="$root/target/release/seloryn.exe"
fi
[ -f "$binary" ] || fail "release binary is missing at target/release; rerun without --skip-build"

python_command=
python_candidates=()
if [ -n "$python_override" ]; then
    python_candidates=("$python_override")
else
    python_candidates=(python3 python)
fi
for candidate in "${python_candidates[@]}"; do
    if command -v "$candidate" >/dev/null 2>&1 && "$candidate" -c 'import sys; assert sys.version_info >= (3, 10)' >/dev/null 2>&1; then
        python_command="$candidate"
        break
    fi
done
[ -n "$python_command" ] || fail 'the Python command selected by the prerequisite doctor is no longer runnable'

printf '%s\n' '' '> target/release/seloryn doctor examples/projects/json-config --format=json'
"$binary" doctor "$root/examples/projects/json-config" --format=json

printf '%s\n' '' '> target/release/seloryn check examples/tensors/tensor-matmul.sely'
"$binary" check "$root/examples/tensors/tensor-matmul.sely"

printf '%s\n' '' '> target/release/seloryn run examples/tensors/tensor-matmul.sely'
"$binary" run "$root/examples/tensors/tensor-matmul.sely"

printf '%s\n' '' '> python3 scripts/ci/conformance.py --run --suite core'
(cd "$root" && "$python_command" scripts/ci/conformance.py --run --suite core)

printf '%s\n' '' 'SELORYN-QUICKSTART: PASS: build, doctor, check, run, and core conformance completed.'
