use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn run_cli(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(arguments)
        .output()
        .expect("the Seloryn CLI binary should start")
}

fn run_cli_owned(arguments: &[std::ffi::OsString]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_seloryn"))
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
    assert!(stdout.contains("No installer or supported release is available yet"));
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
fn build_and_exec_reproduce_a_project_bundle() {
    let root = std::env::temp_dir().join(format!("seloryn-b1-cli-{}", std::process::id()));
    let source_root = root.join("src");
    let first_output = root.join("first");
    let second_output = root.join("second");
    std::fs::create_dir_all(&source_root).expect("project source root");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"cli-fixture\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n",
    )
    .expect("project manifest");
    std::fs::write(
        source_root.join("main.sely"),
        "fn main() -> Result[Text, Text]:\n    return Ok(\"bundle fixture\")\n",
    )
    .expect("project source");

    let build = |output: &std::path::Path| {
        run_cli_owned(&[
            std::ffi::OsString::from("build"),
            root.clone().into_os_string(),
            std::ffi::OsString::from("--profile"),
            std::ffi::OsString::from("debug"),
            std::ffi::OsString::from("--output"),
            output.to_path_buf().into_os_string(),
        ])
    };
    let first = build(&first_output);
    let second = build(&second_output);
    assert!(first.status.success(), "build stderr: {:?}", first.stderr);
    assert!(second.status.success(), "build stderr: {:?}", second.stderr);
    let first_artifact = first_output.join("seloryn.selyapp");
    let second_artifact = second_output.join("seloryn.selyapp");
    assert_eq!(
        std::fs::read(&first_artifact).expect("first artifact"),
        std::fs::read(&second_artifact).expect("second artifact")
    );
    let executed = run_cli_owned(&["exec".into(), first_artifact.into_os_string()]);
    assert!(
        executed.status.success(),
        "exec stderr: {:?}",
        executed.stderr
    );
    assert_eq!(executed.stdout, b"Ok(\"bundle fixture\")\n");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bundle_reproduces_declared_json_read_capability() {
    let root = std::env::temp_dir().join(format!("seloryn-b1-data-{}", std::process::id()));
    let source_root = root.join("src");
    let data_root = root.join("data");
    let output = root.join("bundle");
    std::fs::create_dir_all(&source_root).expect("project source root");
    std::fs::create_dir_all(&data_root).expect("project data root");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"cli-data-fixture\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = [\"data\"]\n",
    )
    .expect("project manifest");
    std::fs::write(data_root.join("config.json"), "{\"threshold\": 12.5}\n").expect("project data");
    std::fs::write(
        source_root.join("main.sely"),
        "from json import read_file, get, as_f64\n\nfn main() -> Result[F64, JsonIoError]:\n    let document: Json = read_file(\"data/config.json\")?\n    let value: Json = get(document, [\"threshold\"])?\n    let threshold: F64 = as_f64(value)?\n    require threshold > 0.0 else JsonIoError(\"threshold must be positive\")\n    return Ok(threshold)\n",
    )
    .expect("project source");

    let build = run_cli_owned(&[
        "build".into(),
        root.clone().into_os_string(),
        "--profile".into(),
        "release".into(),
        "--output".into(),
        output.clone().into_os_string(),
    ]);
    assert!(build.status.success(), "build stderr: {:?}", build.stderr);
    let artifact = output.join("seloryn.selyapp");
    let executed = run_cli_owned(&["exec".into(), artifact.into_os_string()]);
    assert!(
        executed.status.success(),
        "exec stderr: {:?}",
        executed.stderr
    );
    assert_eq!(executed.stdout, b"12.5\n");
    assert!(executed.stderr.is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn runs_public_json_project_example_with_local_module() {
    let checked = run_cli(&["check", "examples/projects/json-config"]);
    assert!(checked.status.success());
    assert!(checked.stderr.is_empty());

    let output = run_cli(&["run", "examples/projects/json-config"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"configured model: gpt-local\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn runs_public_linear_training_fixture_example() {
    let output = run_cli(&["run", "examples/projects/linear-training"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(
        output.stdout,
        b"{\"holdout_mae_max\":0.05,\"holdout_mae_observed\":0.01893684,\"loss_decreased\":true,\"seed\":7}\n"
    );
    assert!(output.stderr.is_empty());
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
fn run_accepts_unqualified_enum_variant_constructors() {
    let path = temporary_source("enum-constructor");
    fs::write(
        &path,
        "enum Decision:\n    Accepted\n    Rejected(Text)\nfn main() -> Decision:\n    return Rejected(\"needs review\")\n",
    )
    .expect("write enum source");

    let output = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"Decision.Rejected(\"needs review\")\n");
    assert!(output.stderr.is_empty());
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
