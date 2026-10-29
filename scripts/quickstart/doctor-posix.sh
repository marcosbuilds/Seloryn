#!/usr/bin/env bash
set -euo pipefail

fail() {
    printf 'SELORYN-DOCTOR: ERROR: %s\n' "$1" >&2
    exit "${2:-127}"
}

usage() {
    cat >&2 <<'EOF'
usage: doctor-posix.sh [--cargo-command PATH] [--python-command PATH]
                       [--node-command PATH] [--require-node]

Checks the local tools needed by the first-run CLI path. It never builds,
writes project files, accesses the network, or runs Seloryn source.
EOF
    exit 64
}

cargo_command="${SELORYN_CARGO:-cargo}"
python_override="${SELORYN_PYTHON:-}"
node_command="${SELORYN_NODE:-node}"
require_node=false

while [ "$#" -gt 0 ]; do
    case "$1" in
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
        --node-command)
            shift
            [ "$#" -gt 0 ] || usage
            node_command="$1"
            ;;
        --require-node)
            require_node=true
            ;;
        --help|-h)
            usage
            ;;
        *)
            usage
            ;;
    esac
    shift
done

command_available() {
    command -v "$1" >/dev/null 2>&1
}

if ! command_available "$cargo_command"; then
    fail "Rust/Cargo is missing. Install the toolchain declared by rust-toolchain.toml with https://rustup.rs, reopen the shell, and verify with 'cargo --version'."
fi
if ! "$cargo_command" --version >/dev/null 2>&1; then
    fail "Cargo was found but could not run. Check the Rust installation and verify with 'cargo --version'."
fi
if ! command_available rustc || ! rustc --version >/dev/null 2>&1; then
    fail "rustc is missing or cannot run. Install the toolchain declared by rust-toolchain.toml with https://rustup.rs, then verify with 'rustc --version'."
fi

python_command=
python_version=
python_candidates=()
if [ -n "$python_override" ]; then
    python_candidates=("$python_override")
else
    python_candidates=(python3 python)
fi

for candidate in "${python_candidates[@]}"; do
    if ! command_available "$candidate"; then
        continue
    fi
    version=$("$candidate" -c 'import sys; print(f"{sys.version_info[0]}.{sys.version_info[1]}")' 2>/dev/null) || continue
    case "$version" in
        3.[1-9][0-9]*|[4-9].*)
            python_command="$candidate"
            python_version="$version"
            break
            ;;
    esac
done

if [ -z "$python_command" ]; then
    fail "Python 3.10 or newer is required for the conformance command. Install Python from https://www.python.org/downloads/, ensure 'python3' or 'python' is on PATH, and verify with 'python --version'."
fi

printf 'SELORYN-DOCTOR: PASS: cargo=%s, rustc=%s, python=%s (%s)\n' \
    "$cargo_command" "$(rustc --version)" "$python_command" "$python_version"

if [ "$require_node" = true ]; then
    if ! command_available "$node_command" || ! "$node_command" --version >/dev/null 2>&1; then
        fail "Node.js is required for the optional VS Code/editor path. Install Node.js from https://nodejs.org/, reopen the shell, and verify with 'node --version'."
    fi
    if ! command_available npm || ! npm --version >/dev/null 2>&1; then
        fail "npm is required for the optional VS Code/editor path. Install npm with Node.js, reopen the shell, and verify with 'npm --version'."
    fi
    printf 'SELORYN-DOCTOR: PASS: node=%s, npm=%s\n' "$node_command" "$(npm --version)"
elif command_available "$node_command" && command_available npm; then
    printf 'SELORYN-DOCTOR: INFO: Node.js/npm detected; they are not required for this CLI first-run path.\n'
else
    printf 'SELORYN-DOCTOR: INFO: Node.js/npm not detected; that is expected for this CLI-only path. Install them only for the optional VS Code/editor checks.\n'
fi
