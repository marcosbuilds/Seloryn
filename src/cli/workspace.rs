use super::{EXIT_DATA, EXIT_IO, EXIT_OK, OutputFormat};
use crate::diagnostic::{LANGUAGE_VERSION, RUNTIME_PACKAGE};
use crate::json::{self, JsonValue};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const CHANNEL: &str = "verified-preview";
const ABI: &str = "unstable";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitTemplate {
    Minimal,
    TabularMlp,
    SourceMlp,
    TrainableMlp,
}

impl InitTemplate {
    fn as_str(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::TabularMlp => "tabular-mlp",
            Self::SourceMlp => "source-mlp",
            Self::TrainableMlp => "trainable-mlp",
        }
    }
}

pub(crate) fn parse_template(value: &OsString) -> Option<InitTemplate> {
    parse_template_text(value.to_str()?)
}

pub(crate) fn parse_template_text(value: &str) -> Option<InitTemplate> {
    match value {
        "minimal" => Some(InitTemplate::Minimal),
        "tabular-mlp" => Some(InitTemplate::TabularMlp),
        "source-mlp" => Some(InitTemplate::SourceMlp),
        "trainable-mlp" => Some(InitTemplate::TrainableMlp),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct DoctorCheck {
    name: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Clone, Debug)]
struct DoctorIssue {
    code: String,
    severity: &'static str,
    message: String,
}

#[derive(Clone, Debug)]
struct DoctorReport {
    project: String,
    status: &'static str,
    production_ready: bool,
    checks: Vec<DoctorCheck>,
    issues: Vec<DoctorIssue>,
}

pub(crate) fn doctor(path: &Path, format: OutputFormat) -> u8 {
    let report = doctor_report(path);
    let exit = if report.issues.iter().any(|issue| issue.severity == "error") {
        EXIT_DATA
    } else {
        EXIT_OK
    };
    let output = match format {
        OutputFormat::Human => render_doctor_human(&report),
        OutputFormat::Json => format!("{}\n", json::encode(&doctor_json(&report))),
    };
    if exit == EXIT_OK {
        super::write_stdout(&output)
    } else {
        super::write_data_stderr(&output)
    }
}

fn doctor_report(path: &Path) -> DoctorReport {
    let project = path.display().to_string();
    let mut report = DoctorReport {
        project,
        status: "warning",
        production_ready: false,
        checks: Vec::new(),
        issues: Vec::new(),
    };

    if !path.exists() {
        report.status = "error";
        report.issues.push(DoctorIssue {
            code: "SEL-D003".to_owned(),
            severity: "error",
            message: "project directory does not exist".to_owned(),
        });
        return report;
    }
    if !path.is_dir() {
        report.status = "error";
        report.issues.push(DoctorIssue {
            code: "SEL-D004".to_owned(),
            severity: "error",
            message: "doctor expects a project directory".to_owned(),
        });
        return report;
    }

    match crate::project::load(path) {
        Ok(loaded) => {
            report.checks.push(DoctorCheck {
                name: "project",
                status: "pass",
                detail: "manifest, source root, entrypoint and declared capabilities are readable"
                    .to_owned(),
            });
            let diagnostics = crate::language::semantic::check(&loaded.program);
            if diagnostics.is_empty() {
                report.checks.push(DoctorCheck {
                    name: "semantics",
                    status: "pass",
                    detail: "static semantic checks passed without executing the project"
                        .to_owned(),
                });
            } else {
                report.status = "error";
                for diagnostic in diagnostics {
                    report.issues.push(DoctorIssue {
                        code: diagnostic.code.to_owned(),
                        severity: "error",
                        message: diagnostic.message,
                    });
                }
            }
        }
        Err(diagnostic) => {
            report.status = "error";
            report.issues.push(DoctorIssue {
                code: diagnostic.code.to_owned(),
                severity: "error",
                message: diagnostic.message,
            });
        }
    }

    if report.status != "error" {
        report.issues.push(DoctorIssue {
            code: "SEL-D001".to_owned(),
            severity: "warning",
            message: format!(
                "runtime package {RUNTIME_PACKAGE} is on channel {CHANNEL} with ABI={ABI}; production_ready remains false"
            ),
        });
    }
    report
}

fn doctor_json(report: &DoctorReport) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        ("abi".to_owned(), JsonValue::String(ABI.to_owned())),
        ("channel".to_owned(), JsonValue::String(CHANNEL.to_owned())),
        (
            "checks".to_owned(),
            JsonValue::Array(
                report
                    .checks
                    .iter()
                    .map(|check| {
                        JsonValue::Object(BTreeMap::from([
                            ("name".to_owned(), JsonValue::String(check.name.to_owned())),
                            (
                                "status".to_owned(),
                                JsonValue::String(check.status.to_owned()),
                            ),
                            ("detail".to_owned(), JsonValue::String(check.detail.clone())),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "issues".to_owned(),
            JsonValue::Array(
                report
                    .issues
                    .iter()
                    .map(|issue| {
                        JsonValue::Object(BTreeMap::from([
                            ("code".to_owned(), JsonValue::String(issue.code.clone())),
                            (
                                "severity".to_owned(),
                                JsonValue::String(issue.severity.to_owned()),
                            ),
                            (
                                "message".to_owned(),
                                JsonValue::String(issue.message.clone()),
                            ),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "language_version".to_owned(),
            JsonValue::String(LANGUAGE_VERSION.to_owned()),
        ),
        (
            "production_ready".to_owned(),
            JsonValue::Bool(report.production_ready),
        ),
        (
            "project".to_owned(),
            JsonValue::String(report.project.clone()),
        ),
        (
            "runtime_package".to_owned(),
            JsonValue::String(RUNTIME_PACKAGE.to_owned()),
        ),
        (
            "schema".to_owned(),
            JsonValue::String("seloryn.doctor.v1".to_owned()),
        ),
        (
            "status".to_owned(),
            JsonValue::String(report.status.to_owned()),
        ),
    ]))
}

fn render_doctor_human(report: &DoctorReport) -> String {
    let mut output = format!(
        "seloryn doctor schema=seloryn.doctor.v1 project={} status={} production_ready={}\n",
        json::encode(&JsonValue::String(report.project.clone())),
        report.status,
        report.production_ready,
    );
    for check in &report.checks {
        output.push_str(&format!(
            "  {} {}: {}\n",
            check.status, check.name, check.detail
        ));
    }
    for issue in &report.issues {
        output.push_str(&format!(
            "  {} {} {}\n",
            issue.severity, issue.code, issue.message
        ));
    }
    output
}

#[derive(Debug)]
struct InitFailure {
    code: &'static str,
    message: String,
    exit: u8,
}

impl InitFailure {
    fn data(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            exit: EXIT_DATA,
        }
    }

    fn io(message: impl Into<String>) -> Self {
        Self {
            code: "SEL-I003",
            message: message.into(),
            exit: EXIT_IO,
        }
    }
}

pub(crate) fn init(
    path: &Path,
    template: InitTemplate,
    name: Option<&std::ffi::OsStr>,
    format: OutputFormat,
) -> u8 {
    match init_project(path, template, name) {
        Ok(result) => {
            let output = match format {
                OutputFormat::Human => render_init_human(&result),
                OutputFormat::Json => format!("{}\n", json::encode(&init_json(&result))),
            };
            super::write_stdout(&output)
        }
        Err(failure) => {
            let output = match format {
                OutputFormat::Human => {
                    format!("{} {}\n", failure.code, failure.message)
                }
                OutputFormat::Json => format!(
                    "{}\n",
                    json::encode(&JsonValue::Object(BTreeMap::from([
                        (
                            "code".to_owned(),
                            JsonValue::String(failure.code.to_owned()),
                        ),
                        ("message".to_owned(), JsonValue::String(failure.message),),
                        (
                            "schema".to_owned(),
                            JsonValue::String("seloryn.init.v1".to_owned()),
                        ),
                    ])))
                ),
            };
            if failure.exit == EXIT_IO {
                super::write_io_diagnostic(&output)
            } else {
                super::write_data_stderr(&output)
            }
        }
    }
}

struct InitResult {
    project: String,
    name: String,
    template: &'static str,
    files: Vec<String>,
}

fn init_project(
    path: &Path,
    template: InitTemplate,
    requested_name: Option<&std::ffi::OsStr>,
) -> Result<InitResult, InitFailure> {
    let raw_target = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| {
                InitFailure::io(format!("failed to resolve current directory: {error}"))
            })?
            .join(path)
    };
    let file_name = raw_target.file_name().ok_or_else(|| {
        InitFailure::data("SEL-I001", "init target must name a new project directory")
    })?;
    let parent = raw_target.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        InitFailure::io(format!(
            "failed to create init parent `{}`: {error}",
            parent.display()
        ))
    })?;
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|error| InitFailure::io(format!("failed to canonicalize init parent: {error}")))?;
    let target = canonical_parent.join(file_name);
    if target.exists() {
        return Err(InitFailure::data(
            "SEL-I001",
            format!("refusing to overwrite existing path `{}`", target.display()),
        ));
    }
    let project_name = match requested_name {
        Some(value) => value
            .to_str()
            .ok_or_else(|| InitFailure::data("SEL-I002", "project name must be valid UTF-8"))?,
        None => file_name.to_str().ok_or_else(|| {
            InitFailure::data("SEL-I002", "project directory name must be valid UTF-8")
        })?,
    };
    validate_project_name(project_name)?;

    let files = template_files(template, project_name);
    let staging_name = format!(
        ".seloryn-init-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    );
    let staging = canonical_parent.join(staging_name);
    fs::create_dir(&staging).map_err(|error| {
        InitFailure::io(format!(
            "failed to create transactional init directory: {error}"
        ))
    })?;
    let write_result = (|| {
        for (relative, contents) in &files {
            let destination = staging.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    InitFailure::io(format!("failed to create `{}`: {error}", parent.display()))
                })?;
            }
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&destination)
                .map_err(|error| {
                    InitFailure::io(format!("failed to write `{relative}`: {error}"))
                })?;
            file.write_all(contents.as_bytes()).map_err(|error| {
                InitFailure::io(format!("failed to write `{relative}`: {error}"))
            })?;
        }
        Ok::<(), InitFailure>(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let validation = crate::project::load(&staging).and_then(|loaded| {
        let diagnostics = crate::language::semantic::check(&loaded.program);
        diagnostics.into_iter().next().map_or(Ok(()), Err)
    });
    if let Err(diagnostic) = validation {
        let _ = fs::remove_dir_all(&staging);
        return Err(InitFailure::data(
            "SEL-I004",
            format!(
                "generated template failed validation: {}",
                diagnostic.message
            ),
        ));
    }
    if let Err(error) = fs::rename(&staging, &target) {
        let _ = fs::remove_dir_all(&staging);
        return Err(InitFailure::io(format!(
            "failed to commit project directory `{}`: {error}",
            target.display()
        )));
    }

    Ok(InitResult {
        project: display_path(&target),
        name: project_name.to_owned(),
        template: template.as_str(),
        files: files.into_iter().map(|(path, _)| path).collect(),
    })
}

fn display_path(path: &Path) -> String {
    let value = path.display().to_string();
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_owned()
}

fn validate_project_name(name: &str) -> Result<(), InitFailure> {
    if name.is_empty() || name.len() > 64 {
        return Err(InitFailure::data(
            "SEL-I002",
            "project name must contain 1 to 64 bytes",
        ));
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(InitFailure::data(
            "SEL-I002",
            "project name may contain only ASCII letters, numbers, `-`, `_` or `.`",
        ));
    }
    Ok(())
}

fn template_files(template: InitTemplate, name: &str) -> Vec<(String, String)> {
    if template == InitTemplate::TrainableMlp {
        return trainable_mlp_template_files(name);
    }
    let mut files = vec![
        (
            "seloryn.toml".to_owned(),
            match template {
                InitTemplate::Minimal => format!(
                    "[package]\nname = \"{name}\"\nlanguage = \"{LANGUAGE_VERSION}\"\nsource_root = \"src\"\nentry = \"main.sely\"\n"
                ),
                InitTemplate::TabularMlp => format!(
                    "[package]\nname = \"{name}\"\nlanguage = \"{LANGUAGE_VERSION}\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = [\"data\"]\nwrite = [\"checkpoints\"]\n\n[limits]\njson_max_bytes = 1048576\njson_max_depth = 32\n"
                ),
                InitTemplate::SourceMlp => format!(
                    "[package]\nname = \"{name}\"\nlanguage = \"{LANGUAGE_VERSION}\"\nsource_root = \"src\"\nentry = \"main.sely\"\n\n[capabilities]\nread = [\"data\", \"checkpoints\"]\nwrite = [\"checkpoints\"]\n\n[limits]\njson_max_bytes = 1048576\njson_max_depth = 32\n"
                ),
                InitTemplate::TrainableMlp => unreachable!(),
            },
        ),
        (
            "src/main.sely".to_owned(),
            match template {
                InitTemplate::Minimal => {
                    "fn main() -> Text:\n    return \"hello from Seloryn\"\n".to_owned()
                }
                InitTemplate::TabularMlp => "from training import train_mlp\n\nfn main() -> Json:\n    return train_mlp(\"data/train.json\", \"data/test.json\", \"checkpoints/model.json\", seed=17, steps=640, batch_size=3, hidden_size=8, learning_rate=0.02)\n".to_owned(),
                InitTemplate::SourceMlp => source_mlp_main().to_owned(),
                InitTemplate::TrainableMlp => unreachable!(),
            },
        ),
        (
            "README.md".to_owned(),
            match template {
                InitTemplate::Minimal => format!(
                    "# {name}\n\nCreated by `seloryn init --template=minimal`.\n\nValidate it without execution:\n\n```text\nseloryn doctor .\nseloryn check .\n```\n"
                ),
                InitTemplate::TabularMlp => format!(
                    "# {name}\n\nCreated by `seloryn init --template=tabular-mlp`. This is a bounded CPU MLP training starter with explicit data and checkpoint capabilities.\n\n```text\nseloryn doctor .\nseloryn run . --log=human\n```\n"
                ),
                InitTemplate::SourceMlp => format!(
                    "# {name}\n\nCreated by `seloryn init --template=source-mlp`. The model, loss, gradient update, checkpoint and reload path are authored in `src/main.sely`; the runtime supplies bounded tensors, autodiff, JSON and capabilities.\n\n```text\nseloryn doctor .\nseloryn check .\nseloryn run . --log=human\n```\n"
                ),
                InitTemplate::TrainableMlp => unreachable!(),
            },
        ),
    ];
    if template == InitTemplate::TabularMlp {
        files.extend([
            (
                "data/train.json".to_owned(),
                "{\"inputs\":[[-2,-2],[-2,0],[-2,2],[0,-2],[0,0],[0,2],[2,-2],[2,0],[2,2]],\"targets\":[[-5.5],[-1.5],[2.5],[-3.5],[0.5],[4.5],[-1.5],[2.5],[6.5]]}\n".to_owned(),
            ),
            (
                "data/test.json".to_owned(),
                "{\"inputs\":[[-1,-1],[-1,1],[1,-1],[1,1],[3,0]],\"targets\":[[-2.5],[1.5],[-0.5],[3.5],[3.5]]}\n".to_owned(),
            ),
            (
                "checkpoints/README.md".to_owned(),
                "Generated checkpoints are written here by the explicit write capability.\n"
                    .to_owned(),
            ),
        ]);
    }
    if template == InitTemplate::SourceMlp {
        files.extend([
            (
                "data/train.json".to_owned(),
                "{\"inputs\":[-2,-1,-1,0,0,1,1,2,2,3,3,4],\"targets\":[-2.6,-0.8,1.0,2.8,4.6,6.4]}\n".to_owned(),
            ),
            (
                "data/test.json".to_owned(),
                "{\"inputs\":[-1,-0.5,0.5,1,2.5,3.5],\"targets\":[-0.8,1.9,5.5]}\n".to_owned(),
            ),
            (
                "checkpoints/README.md".to_owned(),
                "Generated checkpoints are written here by the source-owned model.\n".to_owned(),
            ),
        ]);
    }
    files
}

fn trainable_mlp_template_files(name: &str) -> Vec<(String, String)> {
    [
        (
            "seloryn.toml",
            include_str!("templates/trainable_mlp/seloryn.toml"),
        ),
        (
            "README.md",
            include_str!("templates/trainable_mlp/README.md"),
        ),
        (
            "src/main.sely",
            include_str!("templates/trainable_mlp/src/main.sely"),
        ),
        (
            "src/app/production.sely",
            include_str!("templates/trainable_mlp/src/app/production.sely"),
        ),
        (
            "src/app/contracts.sely",
            include_str!("templates/trainable_mlp/src/app/contracts.sely"),
        ),
        (
            "src/app/data.sely",
            include_str!("templates/trainable_mlp/src/app/data.sely"),
        ),
        (
            "src/app/model.sely",
            include_str!("templates/trainable_mlp/src/app/model.sely"),
        ),
        (
            "src/app/parameters.sely",
            include_str!("templates/trainable_mlp/src/app/parameters.sely"),
        ),
        (
            "src/app/optim.sely",
            include_str!("templates/trainable_mlp/src/app/optim.sely"),
        ),
        (
            "src/app/checkpoint.sely",
            include_str!("templates/trainable_mlp/src/app/checkpoint.sely"),
        ),
        (
            "src/app/training.sely",
            include_str!("templates/trainable_mlp/src/app/training.sely"),
        ),
        (
            "src/app/evaluation.sely",
            include_str!("templates/trainable_mlp/src/app/evaluation.sely"),
        ),
        (
            "src/app/serving.sely",
            include_str!("templates/trainable_mlp/src/app/serving.sely"),
        ),
        (
            "contracts/dataset.json",
            include_str!("templates/trainable_mlp/contracts/dataset.json"),
        ),
        (
            "contracts/model.json",
            include_str!("templates/trainable_mlp/contracts/model.json"),
        ),
        (
            "contracts/run.json",
            include_str!("templates/trainable_mlp/contracts/run.json"),
        ),
        (
            "data/train.json",
            include_str!("templates/trainable_mlp/data/train.json"),
        ),
        (
            "data/test.json",
            include_str!("templates/trainable_mlp/data/test.json"),
        ),
        (
            "checkpoints/README.md",
            include_str!("templates/trainable_mlp/checkpoints/README.md"),
        ),
    ]
    .into_iter()
    .map(|(path, contents)| (path.to_owned(), contents.replace("__PROJECT_NAME__", name)))
    .collect()
}

fn source_mlp_main() -> &'static str {
    r#"from autograd import gradients
from data import range
from json import as_f64_list, as_i64, as_text, from_f64_list, from_i64, from_text, get, object, read_file, sha256, write_file
from tensor import values
from training import progress

record Dataset:
    inputs: Tensor[F32]
    targets: Tensor[F32]

record Model:
    first: Tensor[F32]
    first_bias: Tensor[F32]
    second: Tensor[F32]
    second_bias: Tensor[F32]

record Report:
    architecture: Text
    steps: I64
    initial_loss: F32
    final_loss: F32
    evaluation_loss: F32
    reloaded_loss: F32
    prediction: F32

fn load_dataset(path: Text, rows: I64) -> Result[Dataset, JsonIoError]:
    let document: Json = read_file(path)?
    let input_values: List[F64] = as_f64_list(get(document, ["inputs"])?)?
    let target_values: List[F64] = as_f64_list(get(document, ["targets"])?)?
    return Ok(Dataset(tensor_f32(input_values, [rows, 2]), tensor_f32(target_values, [rows, 1])))

fn forward(inputs: Tensor[F32], model: Model) -> Tensor[F32]:
    let hidden: Tensor[F32] = tensor_relu(matmul(inputs, model.first) + model.first_bias)
    return matmul(hidden, model.second) + model.second_bias

fn objective(inputs: Tensor[F32], first: Tensor[F32], first_bias: Tensor[F32], second: Tensor[F32], second_bias: Tensor[F32], targets: Tensor[F32]) -> Tensor[F32]:
    let hidden: Tensor[F32] = tensor_relu(matmul(inputs, first) + first_bias)
    let prediction: Tensor[F32] = matmul(hidden, second) + second_bias
    let error: Tensor[F32] = prediction - targets
    return tensor_mean(error * error)

fn evaluate(dataset: Dataset, model: Model) -> F32:
    return tensor_item_f32(objective(dataset.inputs, model.first, model.first_bias, model.second, model.second_bias, dataset.targets))

fn save_model(path: Text, model: Model, steps: I64) -> Result[Unit, JsonIoError]:
    let packed: List[F64] = values(model.first) + values(model.first_bias) + values(model.second) + values(model.second_bias)
    let weights: Json = from_f64_list(packed)
    let weights_sha256: Text = sha256(weights)
    let checkpoint_values: List[Json] = [from_text("seloryn.sely.model.v1"), from_text("custom_relu_mlp"), from_i64(2), from_i64(3), from_i64(1), from_i64(steps), weights, from_text(weights_sha256)]
    let checkpoint: Json = object(["schema", "architecture", "input_size", "hidden_size", "output_size", "steps", "weights", "weights_sha256"], checkpoint_values)?
    return write_file(path, checkpoint)

fn load_model(path: Text) -> Result[Model, JsonIoError]:
    let document: Json = read_file(path)?
    let schema: Text = as_text(get(document, ["schema"])?)?
    require schema == "seloryn.sely.model.v1" else JsonIoError("unsupported Seloryn model checkpoint")
    let input_size: I64 = as_i64(get(document, ["input_size"])?)?
    let hidden_size: I64 = as_i64(get(document, ["hidden_size"])?)?
    let output_size: I64 = as_i64(get(document, ["output_size"])?)?
    require input_size == 2 else JsonIoError("checkpoint input shape does not match the source model")
    require hidden_size == 3 else JsonIoError("checkpoint hidden shape does not match the source model")
    require output_size == 1 else JsonIoError("checkpoint output shape does not match the source model")
    let weights: Json = get(document, ["weights"])?
    let expected_weights_sha256: Text = as_text(get(document, ["weights_sha256"])?)?
    require expected_weights_sha256 == sha256(weights) else JsonIoError("checkpoint weights hash does not match the payload")
    let packed: List[F64] = as_f64_list(weights)?
    let first: Tensor[F32] = tensor_f32([packed[0], packed[1], packed[2], packed[3], packed[4], packed[5]], [2, 3])
    let first_bias: Tensor[F32] = tensor_f32([packed[6], packed[7], packed[8]], [1, 3])
    let second: Tensor[F32] = tensor_f32([packed[9], packed[10], packed[11]], [3, 1])
    let second_bias: Tensor[F32] = tensor_f32([packed[12]], [1, 1])
    return Ok(Model(first, first_bias, second, second_bias))

fn train(train_data: Dataset, test_data: Dataset, steps: I64) -> Result[Report, JsonIoError]:
    let rate: Tensor[F32] = tensor_f32([0.05], [])
    var model: Model = Model(tensor_f32([0.2, -0.1, -0.3, 0.4, 0.1, 0.2], [2, 3]), tensor_f32([0.1, 0.1, 0.1], [1, 3]), tensor_f32([0.5, -0.2, 0.3], [3, 1]), tensor_f32([0.1], [1, 1]))
    let initial_loss: F32 = evaluate(train_data, model)
    for step in range(steps):
        let derivatives: List[Tensor[F32]] = gradients(objective, train_data.inputs, model.first, model.first_bias, model.second, model.second_bias, train_data.targets)
        model = Model(model.first - rate * derivatives[1], model.first_bias - rate * derivatives[2], model.second - rate * derivatives[3], model.second_bias - rate * derivatives[4])
        let live_loss: F32 = evaluate(train_data, model)
        progress("source_owned_relu_mlp", step + 1, steps, live_loss)
    let final_loss: F32 = evaluate(train_data, model)
    let evaluation_loss: F32 = evaluate(test_data, model)
    save_model("checkpoints/model.json", model, steps)?
    let reloaded: Model = load_model("checkpoints/model.json")?
    let reloaded_loss: F32 = evaluate(test_data, reloaded)
    let prediction: F32 = tensor_get_f32(forward(test_data.inputs, reloaded), [0, 0])
    return Ok(Report("source_owned_relu_mlp", steps, initial_loss, final_loss, evaluation_loss, reloaded_loss, prediction))

fn main() -> Result[Report, JsonIoError]:
    let train_data: Dataset = load_dataset("data/train.json", 6)?
    let test_data: Dataset = load_dataset("data/test.json", 3)?
    return train(train_data, test_data, 160)
"#
}

fn init_json(result: &InitResult) -> JsonValue {
    JsonValue::Object(BTreeMap::from([
        (
            "files".to_owned(),
            JsonValue::Array(
                result
                    .files
                    .iter()
                    .map(|file| JsonValue::String(file.clone()))
                    .collect(),
            ),
        ),
        (
            "language_version".to_owned(),
            JsonValue::String(LANGUAGE_VERSION.to_owned()),
        ),
        ("name".to_owned(), JsonValue::String(result.name.clone())),
        (
            "project".to_owned(),
            JsonValue::String(result.project.clone()),
        ),
        (
            "runtime_package".to_owned(),
            JsonValue::String(RUNTIME_PACKAGE.to_owned()),
        ),
        (
            "schema".to_owned(),
            JsonValue::String("seloryn.init.v1".to_owned()),
        ),
        (
            "template".to_owned(),
            JsonValue::String(result.template.to_owned()),
        ),
    ]))
}

fn render_init_human(result: &InitResult) -> String {
    format!(
        "seloryn init schema=seloryn.init.v1 project={} template={} language={} files={}\n",
        json::encode(&JsonValue::String(result.project.clone())),
        result.template,
        LANGUAGE_VERSION,
        result.files.len()
    )
}

#[cfg(test)]
mod tests {
    use super::{InitTemplate, parse_template_text, template_files, validate_project_name};

    #[test]
    fn template_parser_is_strict() {
        assert_eq!(parse_template_text("minimal"), Some(InitTemplate::Minimal));
        assert_eq!(
            parse_template_text("tabular-mlp"),
            Some(InitTemplate::TabularMlp)
        );
        assert_eq!(
            parse_template_text("source-mlp"),
            Some(InitTemplate::SourceMlp)
        );
        assert_eq!(
            parse_template_text("trainable-mlp"),
            Some(InitTemplate::TrainableMlp)
        );
        assert_eq!(parse_template_text("all"), None);
    }

    #[test]
    fn template_files_have_expected_entrypoints() {
        for template in [
            InitTemplate::Minimal,
            InitTemplate::TabularMlp,
            InitTemplate::SourceMlp,
            InitTemplate::TrainableMlp,
        ] {
            let files = template_files(template, "demo");
            assert!(files.iter().any(|(path, _)| path == "seloryn.toml"));
            assert!(files.iter().any(|(path, _)| path == "src/main.sely"));
        }
        assert!(
            template_files(InitTemplate::TabularMlp, "demo")
                .iter()
                .any(|(path, _)| path == "data/train.json")
        );
        assert!(
            template_files(InitTemplate::SourceMlp, "demo")
                .iter()
                .any(|(path, contents)| path == "src/main.sely" && contents.contains("gradients"))
        );
        assert!(
            template_files(InitTemplate::TrainableMlp, "demo")
                .iter()
                .any(|(path, contents)| path == "src/app/training.sely"
                    && contents.contains("gradients"))
        );
    }

    #[test]
    fn project_names_are_bounded_and_ascii() {
        assert!(validate_project_name("ok-name_1").is_ok());
        assert!(validate_project_name("").is_err());
        assert!(validate_project_name("na me").is_err());
    }
}
