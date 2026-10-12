use std::process::Command;

fn run_cli(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(arguments)
        .output()
        .expect("the Seloryn CLI binary should start")
}

#[test]
fn help_is_utf8_and_documents_only_available_commands() {
    let output = run_cli(&["--help"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).expect("help must be UTF-8");
    assert!(stdout.contains("Seloryn — native CLI bootstrap"));
    assert!(stdout.contains("seloryn --help"));
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
