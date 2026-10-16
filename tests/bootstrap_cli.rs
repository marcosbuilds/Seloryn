use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;

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

fn run_cli_with_stdin(arguments: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_seloryn"))
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the Seloryn CLI binary should start");
    child
        .stdin
        .take()
        .expect("child stdin")
        .write_all(input)
        .expect("write child stdin");
    child.wait_with_output().expect("wait for Seloryn CLI")
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
    assert!(stdout.contains("Seloryn — native CLI"));
    assert!(stdout.contains("seloryn --help"));
    assert!(stdout.contains("seloryn check <file.sely|project>"));
    assert!(stdout.contains("seloryn run <file.sely|project>"));
    assert!(stdout.contains("seloryn devices [--format=human|json]"));
    assert!(stdout.contains("seloryn gpu-probe [--format=human|json]"));
    assert!(stdout.contains("--log=off|human|json"));
    assert!(stdout.contains("CPU training remains the verified path"));
}

#[test]
fn version_reports_earned_language_and_package_versions_without_claiming_runtime_release() {
    let output = run_cli(&["--version"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).expect("version must be UTF-8");
    assert_eq!(
        stdout,
        "seloryn 0.08 package=0.8.0 build=local-development status=verified-preview\n"
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
fn rejects_malformed_project_manifest_instead_of_ignoring_it() {
    let root =
        std::env::temp_dir().join(format!("seloryn-invalid-manifest-{}", std::process::id()));
    let source_root = root.join("src");
    fs::create_dir_all(&source_root).expect("project source root");
    fs::write(root.join("seloryn.toml"), "this is not valid TOML\n")
        .expect("invalid project manifest");
    fs::write(
        source_root.join("main.sely"),
        "fn main() -> Unit:\n    return\n",
    )
    .expect("project source");

    for command in ["check", "run"] {
        let output = run_cli_owned(&[command.into(), root.clone().into_os_string()]);
        assert_eq!(
            output.status.code(),
            Some(65),
            "{command} accepted invalid manifest"
        );
        assert!(output.stdout.is_empty(), "{command} wrote a false success");
        let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
        assert!(stderr.contains("SEL-M001"), "{command}: {stderr}");
        assert!(
            stderr.contains("invalid project manifest"),
            "{command}: {stderr}"
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejects_project_language_newer_than_runtime() {
    let root = std::env::temp_dir().join(format!("seloryn-future-language-{}", std::process::id()));
    let source_root = root.join("src");
    fs::create_dir_all(&source_root).expect("project source root");
    fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"future-language\"\nlanguage = \"99.99\"\nsource_root = \"src\"\nentry = \"main.sely\"\n",
    )
    .expect("future language manifest");
    fs::write(
        source_root.join("main.sely"),
        "fn main() -> Unit:\n    return\n",
    )
    .expect("project source");

    let output = run_cli_owned(&["check".into(), root.clone().into_os_string()]);
    let _ = fs::remove_dir_all(root);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("SEL-M001"), "{stderr}");
    assert!(
        stderr.contains("requires a newer Seloryn runtime"),
        "{stderr}"
    );
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
fn training_events_are_opt_in_streamed_and_separate_from_stdout() {
    let human = run_cli(&["run", "examples/projects/linear-training", "--log=human"]);
    assert!(human.status.success(), "stderr: {:?}", human.stderr);
    assert_eq!(
        human.stdout,
        b"{\"holdout_mae_max\":0.05,\"holdout_mae_observed\":0.01893684,\"loss_decreased\":true,\"seed\":7}\n"
    );
    let human_stderr = String::from_utf8(human.stderr).expect("human events must be UTF-8");
    let human_lines = human_stderr.lines().collect::<Vec<_>>();
    assert_eq!(human_lines.len(), 13);
    assert!(human_lines[0].contains("training.start schema=1 operation=train_linear model=linear"));
    assert!(human_lines[0].contains("device=cpu dtype=f64 optimizer=sgd learning_rate=0.02000000"));
    assert!(human_lines[0].contains("parameter_count=2"));
    assert!(human_lines[1].contains("training.step schema=1 model=linear step=1/100"));
    assert!(human_lines[1].contains("updates=6") && human_lines[1].contains("loss_delta="));
    assert!(human_lines[11].contains("training.step schema=1 model=linear step=100/100"));
    assert!(
        human_lines[12].contains("training.complete schema=1 operation=train_linear model=linear")
    );
    assert!(human_lines[12].contains("holdout_mae_observed=0.01893684"));
    assert!(
        human_lines[12].contains("updates=600") && human_lines[12].contains("loss_decreased=true")
    );
    assert!(human_lines[12].contains("weight=") && human_lines[12].contains("bias="));
    assert!(!human_lines[12].contains("holdout_mae_max="));

    let json = run_cli(&["run", "examples/projects/linear-training", "--log=json"]);
    assert!(json.status.success(), "stderr: {:?}", json.stderr);
    assert_eq!(json.stdout, human.stdout);
    let json_stderr = String::from_utf8(json.stderr).expect("JSON events must be UTF-8");
    let json_lines = json_stderr.lines().collect::<Vec<_>>();
    assert_eq!(json_lines.len(), 13);
    assert!(json_lines[0].contains("\"event\":\"training.start\""));
    assert!(json_lines[1].contains("\"event\":\"training.step\""));
    assert!(json_lines[12].contains("\"event\":\"training.complete\""));
    assert!(json_lines[12].contains("\"holdout_mae_observed\":0.01893684"));
    assert!(json_lines[0].contains("\"schema_version\":1"));
    assert!(json_lines[0].contains("\"operation\":\"train_linear\""));
    assert!(json_lines[1].contains("\"updates\":6"));
    assert!(json_lines[1].contains("\"loss_delta\":3.79070370"));
    assert!(json_lines[12].contains("\"updates\":600"));
    assert!(json_lines[12].contains("\"loss_decreased\":true"));
    assert!(json_lines[12].contains("\"weight\":"));
    assert!(json_lines[12].contains("\"bias\":"));
    assert!(!json_lines[12].contains("\"holdout_mae_max\""));
    assert!(json_lines[0].contains("\"seed_semantics\":\"metadata_only\""));
}

#[test]
fn training_rejects_path_escape_and_excessive_steps() {
    let root = std::env::temp_dir().join(format!("seloryn-training-guard-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("training source directory");
    std::fs::create_dir_all(root.join("data")).expect("training data directory");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"training-guard\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = [\"data\"]\n",
    )
    .expect("training manifest");
    std::fs::write(root.join("data/train.json"), "{\"x\":[0,1],\"y\":[0,1]}\n")
        .expect("training data");
    std::fs::write(root.join("data/test.json"), "{\"x\":[0],\"y\":[0]}\n").expect("holdout data");

    std::fs::write(
        root.join("src/main.sely"),
        "from training import train_linear\n\nfn main() -> Json:\n    return train_linear(\"../outside.json\", \"data/test.json\", seed=7, steps=1)\n",
    )
    .expect("escape source");
    let escaped = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    assert_eq!(escaped.status.code(), Some(65));
    let escaped_stderr = String::from_utf8(escaped.stderr).expect("escape diagnostic");
    assert!(escaped_stderr.contains("SEL-S004"));
    assert!(escaped_stderr.contains("outside the granted read root"));

    std::fs::write(
        root.join("src/main.sely"),
        "from training import train_linear\n\nfn main() -> Json:\n    return train_linear(\"data/train.json\", \"data/test.json\", seed=7, steps=1000001)\n",
    )
    .expect("excessive source");
    let excessive = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    assert_eq!(excessive.status.code(), Some(65));
    let excessive_stderr = String::from_utf8(excessive.stderr).expect("step diagnostic");
    assert!(excessive_stderr.contains("SEL-R203"));
    assert!(excessive_stderr.contains("training steps exceed maximum 1000000"));
    assert!(!excessive_stderr.contains("training.start"));

    std::fs::write(root.join("data/large.json"), "x".repeat(1_048_577))
        .expect("oversized training data");
    std::fs::write(
        root.join("src/main.sely"),
        "from training import train_linear\n\nfn main() -> Json:\n    return train_linear(\"data/large.json\", \"data/test.json\", seed=7, steps=1)\n",
    )
    .expect("oversized source");
    let oversized = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    assert_eq!(oversized.status.code(), Some(65));
    let oversized_stderr = String::from_utf8(oversized.stderr).expect("size diagnostic");
    assert!(oversized_stderr.contains("SEL-R203"));
    assert!(oversized_stderr.contains("training dataset"));
    assert!(oversized_stderr.contains("1048576 bytes"));

    let values = (0..20)
        .map(|value| value.to_string())
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        root.join("data/heavy.json"),
        format!("{{\"x\":[{values}],\"y\":[{values}]}}\n"),
    )
    .expect("heavy training data");
    std::fs::write(
        root.join("src/main.sely"),
        "from training import train_linear\n\nfn main() -> Json:\n    return train_linear(\"data/heavy.json\", \"data/test.json\", seed=7, steps=600000)\n",
    )
    .expect("heavy source");
    let heavy = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    assert_eq!(heavy.status.code(), Some(65));
    let heavy_stderr = String::from_utf8(heavy.stderr).expect("work diagnostic");
    assert!(heavy_stderr.contains("SEL-R203"));
    assert!(heavy_stderr.contains("training updates exceed maximum 10000000"));
    assert!(!heavy_stderr.contains("training.start"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn training_rejects_holdout_above_enforced_cap() {
    let root = std::env::temp_dir().join(format!(
        "seloryn-training-holdout-cap-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("src")).expect("training source directory");
    std::fs::create_dir_all(root.join("data")).expect("training data directory");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"training-holdout-cap\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = [\"data\"]\n",
    )
    .expect("training manifest");
    std::fs::write(root.join("data/train.json"), "{\"x\":[0,1],\"y\":[0,1]}\n")
        .expect("training data");
    std::fs::write(root.join("data/test.json"), "{\"x\":[10],\"y\":[0]}\n").expect("holdout data");
    std::fs::write(
        root.join("src/main.sely"),
        "from training import train_linear\n\nfn main() -> Json:\n    return train_linear(\"data/train.json\", \"data/test.json\", seed=17, steps=100)\n",
    )
    .expect("training source");

    let output = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    assert!(stderr.contains("SEL-R203"));
    assert!(stderr.contains("holdout MAE"));
    assert!(stderr.contains("exceeds maximum 0.05"));
    assert!(!stderr.contains("training.complete"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn project_without_declared_read_capability_cannot_read_data() {
    let root = std::env::temp_dir().join(format!("seloryn-no-capability-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("project source directory");
    std::fs::create_dir_all(root.join("data")).expect("project data directory");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"no-capability\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n",
    )
    .expect("project manifest");
    std::fs::write(root.join("data/config.json"), "{\"ok\":true}\n").expect("project data");
    std::fs::write(
        root.join("src/main.sely"),
        "from json import read_file\n\nfn main() -> Result[Json, JsonIoError]:\n    return read_file(\"data/config.json\")\n",
    )
    .expect("project source");

    let output = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    assert_eq!(output.status.code(), Some(65));
    let stderr = String::from_utf8(output.stderr).expect("capability diagnostic");
    assert!(stderr.contains("SEL-S004"));
    assert!(stderr.contains("not inside a declared read capability"));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn checkpoint_validation_is_bounded_and_does_not_claim_resume() {
    let valid = run_cli(&["run", "examples/projects/checkpoint-validation"]);
    assert!(valid.status.success(), "stderr: {:?}", valid.stderr);
    assert!(
        String::from_utf8(valid.stdout)
            .expect("checkpoint output")
            .contains("schema-valid")
    );

    let root =
        std::env::temp_dir().join(format!("seloryn-checkpoint-guard-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).expect("checkpoint source directory");
    std::fs::create_dir_all(root.join("data")).expect("checkpoint data directory");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"checkpoint-guard\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = [\"data\"]\n\n[limits]\njson_max_bytes = 128\njson_max_depth = 16\n",
    )
    .expect("checkpoint manifest");
    std::fs::write(root.join("data/checkpoint.json"), "x".repeat(129))
        .expect("oversized checkpoint");
    std::fs::write(
        root.join("src/main.sely"),
        "from training import load_checkpoint\n\nfn main() -> Result[Json, CheckpointError]:\n    return load_checkpoint(\"data/checkpoint.json\")\n",
    )
    .expect("checkpoint source");

    let output = run_cli_owned(&["run".into(), root.clone().into_os_string()]);
    assert_eq!(output.status.code(), Some(65));
    let stderr = String::from_utf8(output.stderr).expect("checkpoint diagnostic");
    assert!(stderr.contains("SEL-K001"));
    assert!(stderr.contains("128 bytes"));
    assert!(!stderr.contains("panicked at"));
    let _ = std::fs::remove_dir_all(root);
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
    assert!(stderr.starts_with("{\"schema\":\"seloryn.analysis.v1\",\"language_version\":\"0.08\",\"runtime_package\":\"0.8.0\",\"diagnostics\":["));
    assert!(stderr.contains("\"code\":\"SEL-T004\""));
    assert!(stderr.contains("\"start\":{\"line\":2"));
    assert!(stderr.ends_with("]}\n"));
}

#[test]
fn check_json_reports_missing_source_as_structured_diagnostic() {
    let path = std::env::temp_dir().join(format!(
        "seloryn-missing-source-{}-{}.sely",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let _ = fs::remove_file(&path);
    let output = run_cli(&[
        "check",
        path.to_str().expect("temporary path is UTF-8"),
        "--format=json",
    ]);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("JSON diagnostics must be UTF-8");
    assert!(stderr.starts_with("{\"schema\":\"seloryn.analysis.v1\",\"language_version\":\"0.08\",\"runtime_package\":\"0.8.0\",\"diagnostics\":["));
    assert!(stderr.contains("\"code\":\"SEL-C003\""));
    assert!(stderr.contains("\"file\":"));
    assert!(stderr.contains("\"start\":{\"line\":1"));
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
fn json_stdin_is_bounded_and_typed() {
    let path = temporary_source("json-stdin");
    fs::write(
        &path,
        "from json import read_stdin, get, as_text\n\nfn main() -> Result[Text, JsonIoError]:\n    let document: Json = read_stdin()?\n    let value: Json = get(document, [\"name\"])?\n    let name: Text = as_text(value)?\n    return Ok(name)\n",
    )
    .expect("write stdin source");

    let output = run_cli_with_stdin(
        &["run", path.to_str().expect("temporary path is UTF-8")],
        br#"{"name":"Seloryn"}"#,
    );
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(output.stdout, b"Seloryn\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn json_run_output_is_a_stable_result_envelope() {
    let path = temporary_source("json-output");
    fs::write(
        &path,
        "from io import print\n\nfn main() -> Text:\n    print(\"trace\")\n    return \"value\"\n",
    )
    .expect("write JSON output source");

    let output = run_cli(&[
        "run",
        path.to_str().expect("temporary path is UTF-8"),
        "--format=json",
    ]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(
        output.stdout,
        b"{\"stdout\":\"trace\\n\",\"value\":\"value\"}\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn json_run_tensor_value_uses_bounded_metadata() {
    let path = temporary_source("json-tensor-output");
    fs::write(
        &path,
        "fn main() -> Tensor[F32]:\n    return tensor_f32([1.0, 2.0], [2, 1])\n",
    )
    .expect("write tensor JSON output source");

    let output = run_cli(&[
        "run",
        path.to_str().expect("temporary path is UTF-8"),
        "--format=json",
    ]);
    let _ = fs::remove_file(&path);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    assert_eq!(
        output.stdout,
        b"{\"stdout\":\"\",\"value\":{\"dtype\":\"F32\",\"elements\":2,\"shape\":[2,1],\"type\":\"tensor\"}}\n"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn json_stdin_rejects_input_above_the_source_limit() {
    let path = temporary_source("json-stdin-limit");
    fs::write(
        &path,
        "from json import read_stdin\n\nfn main() -> Result[Json, JsonIoError]:\n    return read_stdin()\n",
    )
    .expect("write stdin limit source");

    let output = run_cli_with_stdin(
        &["run", path.to_str().expect("temporary path is UTF-8")],
        &[b' '; 129],
    );
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must be UTF-8");
    assert!(stderr.contains("JSON stdin input exceeds the byte limit"));
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
fn run_returns_nonzero_for_a_typed_result_error() {
    let path = temporary_source("run-result-error");
    fs::write(
        &path,
        "fn main() -> Result[I64, Text]:\n    return Err(\"policy disabled\")\n",
    )
    .expect("write Result error source");

    let output = run_cli(&["run", path.to_str().expect("temporary path is UTF-8")]);
    let json_output = run_cli(&[
        "run",
        path.to_str().expect("temporary path is UTF-8"),
        "--format=json",
    ]);
    let _ = fs::remove_file(&path);
    assert_eq!(output.status.code(), Some(65));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-R127"));
    assert!(stderr.contains("program returned Err(\"policy disabled\")"));
    assert!(stderr.contains(":2:5: SEL-R127"));
    assert_eq!(json_output.status.code(), Some(65));
    assert!(json_output.stdout.is_empty());
    let json_stderr = String::from_utf8(json_output.stderr).expect("JSON must be UTF-8");
    assert!(json_stderr.starts_with("{\"schema\":\"seloryn.analysis.v1\",\"language_version\":\"0.08\",\"runtime_package\":\"0.8.0\",\"diagnostics\":["));
    assert!(json_stderr.contains("\"code\":\"SEL-R127\""));
}

#[test]
fn exec_reports_stable_artifact_diagnostics_for_repeated_result_failures() {
    let root = std::env::temp_dir().join(format!("seloryn-bundle-error-{}", std::process::id()));
    let source_root = root.join("src");
    let output_root = root.join("build");
    std::fs::create_dir_all(&source_root).expect("project source root");
    std::fs::write(
        root.join("seloryn.toml"),
        "[package]\nname = \"bundle-error-fixture\"\nlanguage = \"0.00\"\nsource_root = \"src\"\nentry = \"main.sely\"\n",
    )
    .expect("project manifest");
    std::fs::write(
        source_root.join("main.sely"),
        "fn main() -> Result[Unit, Text]:\n    return Err(\"policy disabled\")\n",
    )
    .expect("failing project source");

    let build = run_cli_owned(&[
        "build".into(),
        root.clone().into_os_string(),
        "--profile".into(),
        "release".into(),
        "--output".into(),
        output_root.clone().into_os_string(),
    ]);
    assert!(build.status.success(), "build stderr: {:?}", build.stderr);
    let artifact = output_root.join("seloryn.selyapp");
    let first = run_cli_owned(&["exec".into(), artifact.clone().into_os_string()]);
    let second = run_cli_owned(&["exec".into(), artifact.clone().into_os_string()]);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(first.status.code(), Some(65));
    assert_eq!(second.status.code(), Some(65));
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stderr, second.stderr);
    let stderr = String::from_utf8(first.stderr).expect("diagnostics must be UTF-8");
    assert!(stderr.contains("SEL-R127"));
    assert!(stderr.contains(":2:5: SEL-R127"));
    assert!(stderr.contains(artifact.to_string_lossy().as_ref()));
    assert!(!stderr.contains("seloryn-exec-"));
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
