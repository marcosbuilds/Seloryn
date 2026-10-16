#!/usr/bin/env bash
set -eu

action=install
binary=
root="${HOME}/.local/seloryn"
expected=
expected_language=
expected_runtime=

fail() { printf 'SEL-I001 %s\n' "$1" >&2; exit 65; }
usage() {
    printf '%s\n' 'usage: install-seloryn-posix.sh install|uninstall|rollback|diagnose-path [--binary PATH] [--root PATH] [--sha256 HEX] [--language-version M.mm] [--runtime-version X.Y.Z]' >&2
    exit 64
}
full_hash() {
    if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | awk '{print tolower($1)}'; return; fi
    if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print tolower($1)}'; return; fi
    fail 'sha256sum or shasum is required'
}
json_result() { printf '%s\n' "$1"; }

while [ "$#" -gt 0 ]; do
    case "$1" in
        install|uninstall|rollback|diagnose-path) action="$1" ;;
        --binary) shift; [ "$#" -gt 0 ] || usage; binary="$1" ;;
        --root) shift; [ "$#" -gt 0 ] || usage; root="$1" ;;
        --sha256) shift; [ "$#" -gt 0 ] || usage; expected=$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]') ;;
        --language-version) shift; [ "$#" -gt 0 ] || usage; expected_language="$1" ;;
        --runtime-version) shift; [ "$#" -gt 0 ] || usage; expected_runtime="$1" ;;
        *) usage ;;
    esac
    shift
done

case "$root" in /|.) fail 'install root may not be a filesystem root' ;; esac
root=$(cd "$root" 2>/dev/null && pwd -P || { mkdir -p "$root"; cd "$root"; pwd -P; })
manifest="$root/seloryn-install.json"
binary_path="$root/bin/seloryn"

case "$action" in
    install)
        [ -n "$binary" ] || fail 'install requires --binary'
        [ -f "$binary" ] || fail "binary is missing: $binary"
        hash=$(full_hash "$binary")
        [ -z "$expected" ] || [ "$hash" = "$expected" ] || fail "SHA-256 mismatch: expected $expected, observed $hash"
        version_output=$("$binary" --version 2>&1) || fail 'binary did not return a canonical Seloryn version record'
        language_version=$(printf '%s\n' "$version_output" | sed -n 's/^seloryn \([^ ]*\) package=\([^ ]*\) .*/\1/p')
        runtime_version=$(printf '%s\n' "$version_output" | sed -n 's/^seloryn \([^ ]*\) package=\([^ ]*\) .*/\2/p')
        [ -n "$language_version" ] && [ -n "$runtime_version" ] || fail 'binary did not return a canonical Seloryn version record'
        [ -z "$expected_language" ] || [ "$language_version" = "$expected_language" ] || fail "language version mismatch: expected $expected_language, observed $language_version"
        [ -z "$expected_runtime" ] || [ "$runtime_version" = "$expected_runtime" ] || fail "runtime package mismatch: expected $expected_runtime, observed $runtime_version"
        mkdir -p "$root/bin" "$root/rollback"
        backup=
        if [ -f "$binary_path" ]; then
            stamp=$(date -u +%Y%m%d%H%M%S)
            backup="rollback/seloryn-$stamp"
            cp -f "$binary_path" "$root/$backup"
        fi
        cp -f "$binary" "$binary_path"
        backup_json=null
        [ -z "$backup" ] || backup_json="\"$backup\""
        escaped_version_output=$(printf '%s' "$version_output" | sed 's/\\/\\\\/g; s/"/\\"/g; s/\r$//')
        printf '{"schema":1,"artifact":"seloryn-cli","platform":"posix","binary_relative":"bin/seloryn","sha256":"%s","build_id":"%.16s","language_version":"%s","runtime_package":"%s","version_output":"%s","backup_relative":%s}\n' "$hash" "$hash" "$language_version" "$runtime_version" "$escaped_version_output" "$backup_json" > "$manifest"
        json_result "{\"action\":\"install\",\"artifact\":\"seloryn-cli\",\"platform\":\"posix\",\"sha256\":\"$hash\",\"language_version\":\"$language_version\",\"runtime_package\":\"$runtime_version\",\"path_entry\":\"$root/bin\"}"
        ;;
    uninstall)
        [ -f "$manifest" ] || fail "installation manifest is missing: $manifest"
        rm -f "$binary_path" "$manifest"
        json_result '{"action":"uninstall","artifact":"seloryn-cli","platform":"posix"}'
        ;;
    rollback)
        [ -f "$manifest" ] || fail "installation manifest is missing: $manifest"
        backup=$(sed -n 's/.*"backup_relative":"\([^"]*\)".*/\1/p' "$manifest")
        [ -n "$backup" ] || fail 'no previous binary is available for rollback'
        [ -f "$root/$backup" ] || fail "rollback binary is missing: $root/$backup"
        cp -f "$root/$backup" "$binary_path"
        hash=$(full_hash "$binary_path")
        json_result "{\"action\":\"rollback\",\"artifact\":\"seloryn-cli\",\"platform\":\"posix\",\"sha256\":\"$hash\"}"
        ;;
    diagnose-path)
        case ":${PATH}:" in *":$root/bin:"*) on_path=true ;; *) on_path=false ;; esac
        json_result "{\"action\":\"diagnose-path\",\"bin\":\"$root/bin\",\"on_path\":$on_path,\"fix\":\"Add $root/bin to PATH for this shell\"}"
        ;;
esac
