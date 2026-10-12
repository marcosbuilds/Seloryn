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
        "seloryn 0.00 build=local-development status=design-only\n"
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
fn check_accepts_utf8_crlf_syntax_and_reports_spans_only_on_failure() {
    let path = temporary_source("valid");
    fs::write(
        &path,
        "fn main() -> Unit:\r\n    let value: I64 = -2 ** 2\r\n    return value\r\n",
    )
    .expect("write valid source");

    let output = run_cli(&["check", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert!(
        String::from_utf8(output.stdout)
            .expect("success output must be UTF-8")
            .contains("parsed")
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
