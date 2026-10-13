use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn run_cli(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(arguments)
        .output()
        .expect("the Seloryn CLI binary should start")
}

fn temporary_source(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("seloryn-check-{name}-{}.sely", std::process::id()))
}

#[test]
fn help_is_utf8_and_documents_only_available_commands() {
    let output = run_cli(&["--help"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).expect("help must be UTF-8");
    assert!(stdout.contains("Seloryn — native CLI bootstrap"));
    assert!(stdout.contains("seloryn --help"));
    assert!(stdout.contains("seloryn check <file.sely>"));
    assert!(stdout.contains("seloryn run <file.sely>"));
    assert!(stdout.contains("not available yet"));
}

#[test]
fn version_reports_language_baseline_without_claiming_a_release() {
    let output = run_cli(&["--version"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).expect("version must be UTF-8");
    assert_eq!(
        stdout,
        "seloryn 0.00 build=local-development status=experimental-implemented\n"
    );
}

#[test]
fn unicode_unsupported_argument_has_structured_failure() {
    let output = run_cli(&["entrada-入口"]);
    assert_eq!(output.status.code(), Some(64));

    let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-C001"));
    assert!(stderr.contains("entrada-入口"));
}

#[test]
fn extra_arguments_are_not_silently_ignored() {
    let output = run_cli(&["--help", "arquivo com espacos.sely"]);
    assert_eq!(output.status.code(), Some(64));

    let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("arquivo com espacos.sely"));
}

#[test]
fn check_accepts_utf8_crlf_syntax_and_semantics() {
    let path = temporary_source("valid");
    fs::write(
        &path,
        "fn main() -> I64:\r\n    let value: I64 = -2 ** 2\r\n    return value\r\n",
    )
    .expect("write valid source");

    let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(
        String::from_utf8(output.stdout)
            .expect("success output must be UTF-8")
            .contains("checked")
    );
}

#[test]
fn check_rejects_tabs_with_a_lexical_diagnostic() {
    let path = temporary_source("tab");
    fs::write(&path, b"fn main():\n\treturn 1\n").expect("write tab source");

    let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-L002"));
    assert!(stderr.contains(":2:1:"));
}

#[test]
fn check_rejects_invalid_utf8_before_parsing() {
    let path = temporary_source("bytes");
    fs::write(&path, [b'f', b'n', b' ', 0xff, b'\n']).expect("write invalid UTF-8 source");

    let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-L001"));
    assert!(stderr.contains(":1:1:"));
}

#[test]
fn check_rejects_reserved_syntax_explicitly() {
    let path = temporary_source("reserved");
    fs::write(&path, "class Example:\n").expect("write reserved source");

    let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-P003"));
    assert!(stderr.contains("reserved syntax `class`"));
}

#[test]
fn check_accepts_result_and_exhaustive_match_semantics() {
    let path = temporary_source("result-match");
    fs::write(
        &path,
        "enum Flag:\r\n    On\r\n    Off\r\nfn main(flag: Flag) -> Unit:\r\n    match flag:\r\n        case On:\r\n            return\r\n        case Off:\r\n            return\r\n",
    )
    .expect("write semantic source");

    let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(
        String::from_utf8(output.stdout)
            .expect("success output must be UTF-8")
            .contains("syntax + semantics")
    );
}

#[test]
fn check_rejects_semantic_name_type_and_match_errors() {
    let cases = [
        (
            "undefined",
            "fn main() -> I64:\n    return missing\n",
            "SEL-M002",
        ),
        (
            "condition",
            "fn main() -> Unit:\n    if 1:\n        return\n",
            "SEL-T004",
        ),
        (
            "match",
            "enum Flag:\n    On\n    Off\nfn main(flag: Flag) -> Unit:\n    match flag:\n        case On:\n            return\n",
            "SEL-R003",
        ),
        (
            "import",
            "import seloryn.tensor\nfn main() -> Unit:\n    return\n",
            "SEL-M001",
        ),
    ];

    for (name, source, code) in cases {
        let path = temporary_source(name);
        fs::write(&path, source).expect("write semantic negative source");
        let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
        let _ = fs::remove_file(&path);
        assert_eq!(output.status.code(), Some(65), "case {name}");
        assert!(output.stdout.is_empty(), "case {name} wrote false success");
        let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
        assert!(stderr.contains(code), "case {name}: {stderr}");
    }
}

#[test]
fn check_json_has_one_structured_document_and_no_stdout() {
    let path = temporary_source("json-diagnostic");
    fs::write(&path, "fn main() -> Unit:\n    if 1:\n        return\n")
        .expect("write JSON diagnostic source");

    let output = run_cli(&[
        "check",
        path.to_str().expect("temporary path is UTF-8"),
        "--format=json",
    ]);
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("JSON diagnostics must be UTF-8");
    assert!(stderr.starts_with("{\"diagnostics\":["));
    assert!(stderr.contains("\"code\":\"SEL-T004\""));
    assert!(stderr.contains("\"start\":{\"line\":2"));
    assert!(stderr.ends_with("]}\n"));
}

#[test]
fn quiet_check_suppresses_success_line() {
    let path = temporary_source("quiet");
    fs::write(&path, "fn main() -> Unit:\n    return\n").expect("write quiet source");

    let output = run_cli(&[
        "check",
        path.to_str().expect("temporary path is UTF-8"),
        "--quiet",
    ]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn io_print_is_explicit_and_does_not_duplicate_unit() {
    let path = temporary_source("io-print");
    fs::write(
        &path,
        "from io import print\n\nfn main() -> Unit:\n    print(\"hello\")\n",
    )
    .expect("write print source");

    let output = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn run_executes_crlf_unicode_source_and_prints_a_value() {
    let path = temporary_source("run-unicode");
    fs::write(
        &path,
        "fn saudação() -> Text:\r\n    return \"olá\"\r\nfn main() -> Text:\r\n    return saudação()\r\n",
    )
    .expect("write runnable source");

    let output = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        String::from_utf8(output.stdout).expect("result must be UTF-8"),
        "\"olá\"\n"
    );
}

#[test]
fn run_accepts_a_source_path_with_spaces() {
    let path = std::env::temp_dir().join(format!("seloryn run path {}.sely", std::process::id()));
    fs::write(&path, "fn main() -> I64:\n    return 42\n").expect("write spaced-path source");

    let output = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).expect("result must be UTF-8"),
        "42\n"
    );
}

#[test]
fn run_does_not_execute_after_a_static_error() {
    let path = temporary_source("run-static-error");
    fs::write(&path, "fn main() -> I64:\n    return missing\n").expect("write invalid source");

    let output = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .expect("diagnostics must be UTF-8")
            .contains("SEL-M002")
    );
}

#[test]
fn run_reports_the_same_structured_runtime_error_each_time() {
    let path = temporary_source("run-runtime-error");
    fs::write(&path, "fn main() -> I64:\n    return 1 / 0\n").expect("write runtime-error source");

    let first = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let second = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert_eq!(first.status.code(), Some(65));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stderr, second.stderr);
    let stderr = String::from_utf8(first.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-R118"));
    assert!(stderr.contains("stack main"));
}

#[test]
fn run_rejects_global_tensor_budget_without_panicking() {
    let path = temporary_source("global-tensor-budget");
    fs::write(
        &path,
        "fn main() -> Tensor[F32]:\n    let first: Tensor[F32] = tensor_zeros_f32([6291456])\n    let second: Tensor[F32] = tensor_zeros_f32([6291456])\n    return tensor_zeros_f32([4194305])\n",
    )
    .expect("write global-budget source");

    let first = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let second = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert_eq!(first.status.code(), Some(65));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stderr, second.stderr);
    let stderr = String::from_utf8(first.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-R209"));
    assert!(stderr.contains("stack main"));
}
