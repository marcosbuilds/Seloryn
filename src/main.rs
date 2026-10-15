#![allow(clippy::result_large_err)]

mod autograd;
mod builtins;
mod diagnostic;
mod gpu;
mod ir;
mod json;
mod lexer;
mod parser;
mod project;
mod runtime;
mod semantic;
mod tensor;
mod tensor_autograd;

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

pub(crate) const LANGUAGE_VERSION: &str = "0.05";
const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEVELOPMENT_BUILD_ID: &str = "local-development";
const EXIT_OK: u8 = 0;
const EXIT_USAGE: u8 = 64;
const EXIT_DATA: u8 = 65;
const EXIT_IO: u8 = 74;

const HELP_TEXT: &str = "Seloryn — native CLI\n\nUsage:\n  seloryn --help\n  seloryn --version\n  seloryn devices [--format=human|json]\n  seloryn gpu-probe [--format=human|json]\n  seloryn check <file.sely|project> [--format=human|json] [--quiet]\n  seloryn run <file.sely|project> [--format=human|json] [--log=off|human|json]\n  seloryn build <project> --profile debug|release --output <directory>\n  seloryn exec <bundle.selyapp|directory>\n\n`check` validates UTF-8, layout, syntax and the supported semantic core.\n`run` executes bounded scalar, CPU tensor, reverse-mode, Seloryn-authored and CPU MLP training profiles.\n`run --format=json` writes a stable `{stdout,value}` result envelope to stdout on success.\n`from json import read_stdin` accepts one bounded JSON document from stdin.\n`--log` is off by default; when enabled, training progress is streamed and flushed to stderr.\n`devices` enumerates the compiled hardware adapters without selecting one for training.\n`gpu-probe` submits a bounded compute shader and verifies readback on a physical non-fallback adapter.\n`build` creates a deterministic `.selyapp` bundle; `exec` runs one.\nCPU training remains the verified path; GPU probing is available, but GPU training, distributed execution and installer support remain unavailable.\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogFormat {
    Off,
    Human,
    Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Options {
    format: OutputFormat,
    color: ColorMode,
    log: LogFormat,
    quiet: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            format: OutputFormat::Human,
            color: ColorMode::Auto,
            log: LogFormat::Off,
            quiet: false,
        }
    }
}

enum Action {
    Help,
    Version,
    Devices {
        format: OutputFormat,
    },
    GpuProbe {
        format: OutputFormat,
    },
    Check {
        path: OsString,
        options: Options,
    },
    Run {
        path: OsString,
        options: Options,
    },
    Build {
        path: OsString,
        profile: OsString,
        output: OsString,
    },
    Exec {
        path: OsString,
    },
    Unsupported(String),
}

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    ExitCode::from(run(&args))
}

fn run(args: &[OsString]) -> u8 {
    match classify(args) {
        Action::Help => write_stdout(HELP_TEXT),
        Action::Version => write_stdout(&format!(
            "seloryn {LANGUAGE_VERSION} package={PACKAGE_VERSION} build={DEVELOPMENT_BUILD_ID} status=verified-preview\n"
        )),
        Action::Devices { format } => devices(format),
        Action::GpuProbe { format } => gpu_probe(format),
        Action::Check { path, options } => check_path(PathBuf::from(path), options),
        Action::Run { path, options } => run_path(PathBuf::from(path), options),
        Action::Build {
            path,
            profile,
            output,
        } => build_path(
            PathBuf::from(path),
            profile.to_string_lossy().into_owned(),
            PathBuf::from(output),
        ),
        Action::Exec { path } => exec_path(PathBuf::from(path)),
        Action::Unsupported(command) => write_stderr(&format!(
            "SEL-C001 unsupported command or argument: {command}\n"
        )),
    }
}

fn classify(args: &[OsString]) -> Action {
    match args {
        [] => Action::Help,
        [argument] if argument == "--help" || argument == "-h" => Action::Help,
        [argument] if argument == "--version" => Action::Version,
        [command, rest @ ..] if command == "devices" => parse_device_command("devices", rest),
        [command, rest @ ..] if command == "gpu-probe" => parse_device_command("gpu-probe", rest),
        [command, rest @ ..] if command == "check" => parse_command("check", rest),
        [command, rest @ ..] if command == "run" => parse_command("run", rest),
        [command, rest @ ..] if command == "build" => parse_build(rest),
        [command, rest @ ..] if command == "exec" => parse_exec(rest),
        _ => unsupported_arguments(args),
    }
}

fn parse_device_command(command: &str, args: &[OsString]) -> Action {
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--format" {
            let Some(value) = args.get(index + 1) else {
                return unsupported_arguments(args);
            };
            let Some(parsed) = parse_format(value) else {
                return unsupported_arguments(args);
            };
            format = parsed;
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(parsed) = parse_format_text(value) else {
                return unsupported_arguments(args);
            };
            format = parsed;
        } else {
            return unsupported_arguments(args);
        }
        index += 1;
    }
    match command {
        "devices" => Action::Devices { format },
        "gpu-probe" => Action::GpuProbe { format },
        _ => Action::Unsupported(command.to_owned()),
    }
}

fn parse_build(args: &[OsString]) -> Action {
    let mut path = None;
    let mut profile = None;
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--profile" || argument == "--output" {
            let Some(value) = args.get(index + 1) else {
                return unsupported_arguments(args);
            };
            if argument == "--profile" {
                profile = Some(value.clone());
            } else {
                output = Some(value.clone());
            }
            index += 1;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with("--profile="))
        {
            profile = argument
                .to_str()
                .and_then(|value| value.strip_prefix("--profile="))
                .map(OsString::from);
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with("--output="))
        {
            output = argument
                .to_str()
                .and_then(|value| value.strip_prefix("--output="))
                .map(OsString::from);
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
            || path.replace(argument.clone()).is_some()
        {
            return unsupported_arguments(args);
        }
        index += 1;
    }
    match (path, profile, output) {
        (Some(path), Some(profile), Some(output)) => Action::Build {
            path,
            profile,
            output,
        },
        _ => unsupported_arguments(args),
    }
}

fn parse_exec(args: &[OsString]) -> Action {
    if args.len() == 1 {
        Action::Exec {
            path: args[0].clone(),
        }
    } else {
        unsupported_arguments(args)
    }
}

fn parse_command(command: &str, args: &[OsString]) -> Action {
    let mut options = Options::default();
    let mut path = None;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--quiet" {
            options.quiet = true;
        } else if argument == "--format" || argument == "--color" || argument == "--log" {
            let Some(value) = args.get(index + 1) else {
                return unsupported_arguments(args);
            };
            if argument == "--format" {
                let Some(format) = parse_format(value) else {
                    return unsupported_arguments(args);
                };
                options.format = format;
            } else if argument == "--color" {
                let Some(color) = parse_color(value) else {
                    return unsupported_arguments(args);
                };
                options.color = color;
            } else {
                let Some(log) = parse_log(value) else {
                    return unsupported_arguments(args);
                };
                options.log = log;
            }
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(format) = parse_format_text(value) else {
                return unsupported_arguments(args);
            };
            options.format = format;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--color="))
        {
            let Some(color) = parse_color_text(value) else {
                return unsupported_arguments(args);
            };
            options.color = color;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--log="))
        {
            let Some(log) = parse_log_text(value) else {
                return unsupported_arguments(args);
            };
            options.log = log;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
            || path.replace(argument.clone()).is_some()
        {
            return unsupported_arguments(args);
        }
        index += 1;
    }
    let Some(path) = path else {
        return unsupported_arguments(args);
    };
    match command {
        "check" => Action::Check { path, options },
        "run" => Action::Run { path, options },
        _ => Action::Unsupported(command.to_owned()),
    }
}

fn unsupported_arguments(args: &[OsString]) -> Action {
    Action::Unsupported(
        args.iter()
            .map(|argument| display_argument(argument))
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn parse_format(argument: &OsString) -> Option<OutputFormat> {
    parse_format_text(argument.to_str()?)
}

fn parse_format_text(value: &str) -> Option<OutputFormat> {
    match value {
        "human" => Some(OutputFormat::Human),
        "json" => Some(OutputFormat::Json),
        _ => None,
    }
}

fn parse_color(argument: &OsString) -> Option<ColorMode> {
    parse_color_text(argument.to_str()?)
}

fn parse_color_text(value: &str) -> Option<ColorMode> {
    match value {
        "auto" => Some(ColorMode::Auto),
        "always" => Some(ColorMode::Always),
        "never" => Some(ColorMode::Never),
        _ => None,
    }
}

fn parse_log(argument: &OsString) -> Option<LogFormat> {
    parse_log_text(argument.to_str()?)
}

fn parse_log_text(value: &str) -> Option<LogFormat> {
    match value {
        "off" => Some(LogFormat::Off),
        "human" => Some(LogFormat::Human),
        "json" => Some(LogFormat::Json),
        _ => None,
    }
}

struct CliEventSink {
    format: LogFormat,
    failure: Option<io::Error>,
}

impl CliEventSink {
    fn new(format: LogFormat) -> Self {
        Self {
            format,
            failure: None,
        }
    }

    fn take_failure(&mut self) -> Option<io::Error> {
        self.failure.take()
    }

    fn write_line(&mut self, line: String) {
        if self.failure.is_some() || self.format == LogFormat::Off {
            return;
        }
        let mut stderr = io::stderr();
        if let Err(error) = stderr
            .write_all(format!("{line}\n").as_bytes())
            .and_then(|()| stderr.flush())
        {
            self.failure = Some(error);
        }
    }
}

impl runtime::EventSink for CliEventSink {
    fn emit(&mut self, event: runtime::RuntimeEvent) {
        let line = match self.format {
            LogFormat::Off => return,
            LogFormat::Human => render_runtime_event_human(&event),
            LogFormat::Json => render_runtime_event_json(&event),
        };
        self.write_line(line);
    }
}

fn render_runtime_event_human(event: &runtime::RuntimeEvent) -> String {
    match event {
        runtime::RuntimeEvent::Started {
            model,
            train_path,
            test_path,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
        } => format!(
            "seloryn training.start schema=1 operation=train_linear model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} train={} test={} train_samples={} test_samples={} steps={} seed_metadata={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            json::encode(&json::JsonValue::String(train_path.clone())),
            json::encode(&json::JsonValue::String(test_path.clone())),
            train_samples,
            test_samples,
            steps,
            seed,
        ),
        runtime::RuntimeEvent::Step {
            model,
            step,
            steps,
            updates,
            loss,
            loss_delta,
        } => format!(
            "seloryn training.step schema=1 model={} step={}/{} updates={} loss={loss:.8} loss_delta={loss_delta:.8}",
            model, step, steps, updates
        ),
        runtime::RuntimeEvent::Completed {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            initial_loss,
            final_loss,
            loss_delta,
            loss_decreased,
            holdout_mae_observed,
            weight,
            bias,
        } => format!(
            "seloryn training.complete schema=1 operation=train_linear model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} updates={} steps={} initial_loss={initial_loss:.8} final_loss={final_loss:.8} loss_delta={loss_delta:.8} loss_decreased={} holdout_mae_observed={holdout_mae_observed:.8} weight={weight:.8} bias={bias:.8} seed_metadata={}",
            model, device, dtype, optimizer, parameter_count, updates, steps, loss_decreased, seed
        ),
        runtime::RuntimeEvent::MlpStarted {
            model,
            train_path,
            test_path,
            checkpoint_path,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            resumed_from,
        } => format!(
            "seloryn training.start schema=1 operation=train_mlp model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} batch_size={} hidden_size={} train={} test={} checkpoint={} train_samples={} test_samples={} steps={} seed={} resumed_from={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            batch_size,
            hidden_size,
            json::encode(&json::JsonValue::String(train_path.clone())),
            json::encode(&json::JsonValue::String(test_path.clone())),
            json::encode(&json::JsonValue::String(checkpoint_path.clone())),
            train_samples,
            test_samples,
            steps,
            seed,
            resumed_from,
        ),
        runtime::RuntimeEvent::MlpStep {
            model,
            step,
            steps,
            updates,
            epoch,
            loss,
            evaluation_loss,
        } => format!(
            "seloryn training.step schema=1 operation=train_mlp model={} step={}/{} updates={} epoch={} loss={loss:.8} evaluation_loss={evaluation_loss:.8}",
            model, step, steps, updates, epoch
        ),
        runtime::RuntimeEvent::MlpCompleted {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            initial_loss,
            final_loss,
            evaluation_loss,
            loss_decreased,
            resumed_from,
            checkpoint_sha256,
        } => format!(
            "seloryn training.complete schema=1 operation=train_mlp model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} batch_size={} hidden_size={} updates={} steps={} initial_loss={initial_loss:.8} final_loss={final_loss:.8} evaluation_loss={evaluation_loss:.8} loss_decreased={} seed={} resumed_from={} checkpoint_sha256={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            batch_size,
            hidden_size,
            updates,
            steps,
            loss_decreased,
            seed,
            resumed_from,
            checkpoint_sha256,
        ),
    }
}

fn render_runtime_event_json(event: &runtime::RuntimeEvent) -> String {
    let mut object = BTreeMap::new();
    match event {
        runtime::RuntimeEvent::Started {
            model,
            train_path,
            test_path,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.start".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_linear".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "train_path".to_owned(),
                json::JsonValue::String(train_path.clone()),
            );
            object.insert(
                "test_path".to_owned(),
                json::JsonValue::String(test_path.clone()),
            );
            object.insert(
                "train_samples".to_owned(),
                json::JsonValue::Number(train_samples.to_string()),
            );
            object.insert(
                "test_samples".to_owned(),
                json::JsonValue::Number(test_samples.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "seed_semantics".to_owned(),
                json::JsonValue::String("metadata_only".to_owned()),
            );
            object.insert(
                "step_sampling".to_owned(),
                json::JsonValue::String("first,10_percent,final".to_owned()),
            );
        }
        runtime::RuntimeEvent::Step {
            model,
            step,
            steps,
            updates,
            loss,
            loss_delta,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.step".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("step".to_owned(), json::JsonValue::Number(step.to_string()));
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert(
                "loss".to_owned(),
                json::JsonValue::Number(format!("{loss:.8}")),
            );
            object.insert(
                "loss_delta".to_owned(),
                json::JsonValue::Number(format!("{loss_delta:.8}")),
            );
        }
        runtime::RuntimeEvent::Completed {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            initial_loss,
            final_loss,
            loss_delta,
            loss_decreased,
            holdout_mae_observed,
            weight,
            bias,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.complete".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_linear".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "seed_semantics".to_owned(),
                json::JsonValue::String("metadata_only".to_owned()),
            );
            object.insert(
                "initial_loss".to_owned(),
                json::JsonValue::Number(format!("{initial_loss:.8}")),
            );
            object.insert(
                "final_loss".to_owned(),
                json::JsonValue::Number(format!("{final_loss:.8}")),
            );
            object.insert(
                "loss_delta".to_owned(),
                json::JsonValue::Number(format!("{loss_delta:.8}")),
            );
            object.insert(
                "loss_decreased".to_owned(),
                json::JsonValue::Bool(*loss_decreased),
            );
            object.insert(
                "holdout_mae_observed".to_owned(),
                json::JsonValue::Number(format!("{holdout_mae_observed:.8}")),
            );
            object.insert(
                "weight".to_owned(),
                json::JsonValue::Number(format!("{weight:.8}")),
            );
            object.insert(
                "bias".to_owned(),
                json::JsonValue::Number(format!("{bias:.8}")),
            );
        }
        runtime::RuntimeEvent::MlpStarted {
            model,
            train_path,
            test_path,
            checkpoint_path,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            resumed_from,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.start".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "batch_size".to_owned(),
                json::JsonValue::Number(batch_size.to_string()),
            );
            object.insert(
                "hidden_size".to_owned(),
                json::JsonValue::Number(hidden_size.to_string()),
            );
            object.insert(
                "train_path".to_owned(),
                json::JsonValue::String(train_path.clone()),
            );
            object.insert(
                "test_path".to_owned(),
                json::JsonValue::String(test_path.clone()),
            );
            object.insert(
                "checkpoint_path".to_owned(),
                json::JsonValue::String(checkpoint_path.clone()),
            );
            object.insert(
                "train_samples".to_owned(),
                json::JsonValue::Number(train_samples.to_string()),
            );
            object.insert(
                "test_samples".to_owned(),
                json::JsonValue::Number(test_samples.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "resumed_from".to_owned(),
                json::JsonValue::Number(resumed_from.to_string()),
            );
            object.insert(
                "seed_semantics".to_owned(),
                json::JsonValue::String("deterministic_cpu_stream".to_owned()),
            );
        }
        runtime::RuntimeEvent::MlpStep {
            model,
            step,
            steps,
            updates,
            epoch,
            loss,
            evaluation_loss,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.step".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("step".to_owned(), json::JsonValue::Number(step.to_string()));
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert(
                "epoch".to_owned(),
                json::JsonValue::Number(epoch.to_string()),
            );
            object.insert(
                "loss".to_owned(),
                json::JsonValue::Number(format!("{loss:.8}")),
            );
            object.insert(
                "evaluation_loss".to_owned(),
                json::JsonValue::Number(format!("{evaluation_loss:.8}")),
            );
        }
        runtime::RuntimeEvent::MlpCompleted {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            initial_loss,
            final_loss,
            evaluation_loss,
            loss_decreased,
            resumed_from,
            checkpoint_sha256,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.complete".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "batch_size".to_owned(),
                json::JsonValue::Number(batch_size.to_string()),
            );
            object.insert(
                "hidden_size".to_owned(),
                json::JsonValue::Number(hidden_size.to_string()),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "initial_loss".to_owned(),
                json::JsonValue::Number(format!("{initial_loss:.8}")),
            );
            object.insert(
                "final_loss".to_owned(),
                json::JsonValue::Number(format!("{final_loss:.8}")),
            );
            object.insert(
                "evaluation_loss".to_owned(),
                json::JsonValue::Number(format!("{evaluation_loss:.8}")),
            );
            object.insert(
                "loss_decreased".to_owned(),
                json::JsonValue::Bool(*loss_decreased),
            );
            object.insert(
                "resumed_from".to_owned(),
                json::JsonValue::Number(resumed_from.to_string()),
            );
            object.insert(
                "checkpoint_sha256".to_owned(),
                json::JsonValue::String(checkpoint_sha256.clone()),
            );
        }
    }
    json::encode(&json::JsonValue::Object(object))
}

fn display_argument(argument: &OsStr) -> String {
    let Some(argument) = argument.to_str() else {
        return "<non-UTF-8 argument>".to_owned();
    };

    let mut displayed = String::with_capacity(argument.len());
    for character in argument.chars() {
        match character {
            '\t' => displayed.push_str("\\t"),
            '\r' => displayed.push_str("\\r"),
            '\n' => displayed.push_str("\\n"),
            character if character.is_control() => {
                displayed.push_str(&format!("\\u{{{:x}}}", character as u32));
            }
            character => displayed.push(character),
        }
    }
    displayed
}

fn check_path(path: PathBuf, options: Options) -> u8 {
    if path.is_dir() {
        return check_project_path(&path, options);
    }
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return check_json_data_path(&path, options);
    }
    let label = display_argument(path.as_os_str());
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-C003",
                format!("failed to read source `{label}`: {error}"),
                diagnostic::Span::new(0, 0),
            );
            return write_diagnostics(&path, "", &[diagnostic], options);
        }
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => {
            let offset = error.utf8_error().valid_up_to();
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-L001",
                format!("source is not valid UTF-8 at byte offset {offset}"),
                diagnostic::Span::new(offset, offset.saturating_add(1)),
            );
            return write_diagnostics(&path, "", &[diagnostic], options);
        }
    };

    let parsed = parser::parse(&source);
    if parsed.diagnostics.is_empty() {
        let Some(program) = parsed.program else {
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-C004",
                "parser returned no program without diagnostics",
                diagnostic::Span::new(0, 0),
            );
            return write_diagnostics(&path, &source, &[diagnostic], options);
        };
        let semantic_diagnostics = semantic::check(&program);
        if semantic_diagnostics.is_empty() {
            if options.quiet {
                return EXIT_OK;
            }
            return write_stdout(&format!("checked {label} (syntax + semantics)\n"));
        }
        return write_diagnostics(&path, &source, &semantic_diagnostics, options);
    }
    write_diagnostics(&path, &source, &parsed.diagnostics, options)
}

fn devices(format: OutputFormat) -> u8 {
    let adapters = gpu::enumerate_adapters();
    match format {
        OutputFormat::Human => {
            if adapters.is_empty() {
                return write_stdout("seloryn devices count=0\n");
            }
            let output = adapters
                .iter()
                .enumerate()
                .map(|(index, adapter)| {
                    format!(
                        "seloryn device index={index} type={} backend={} name={} vendor=0x{:08x} device=0x{:08x} driver={} driver_info={}\n",
                        adapter.device_type,
                        adapter.backend,
                        json::encode(&json::JsonValue::String(adapter.name.clone())),
                        adapter.vendor,
                        adapter.device,
                        json::encode(&json::JsonValue::String(adapter.driver.clone())),
                        json::encode(&json::JsonValue::String(adapter.driver_info.clone()))
                    )
                })
                .collect::<String>();
            write_stdout(&output)
        }
        OutputFormat::Json => {
            let result = json::JsonValue::Object(BTreeMap::from([
                (
                    "devices".to_owned(),
                    json::JsonValue::Array(adapters.iter().map(adapter_json).collect()),
                ),
                (
                    "count".to_owned(),
                    json::JsonValue::Number(adapters.len().to_string()),
                ),
            ]));
            write_stdout(&format!("{}\n", json::encode(&result)))
        }
    }
}

fn gpu_probe(format: OutputFormat) -> u8 {
    match gpu::run_probe() {
        Ok(report) => {
            let result = probe_json(&report);
            match format {
                OutputFormat::Human => write_stdout(&format!(
                    "seloryn gpu-probe schema=1 device={} backend={} elements={} max_abs_error={:.9e} result={}\n",
                    json::encode(&json::JsonValue::String(report.adapter.name)),
                    report.adapter.backend,
                    report.elements,
                    report.max_abs_error,
                    json::encode(result.get("result").expect("probe result"))
                )),
                OutputFormat::Json => write_stdout(&format!(
                    "{}\n",
                    json::encode(&json::JsonValue::Object(result))
                )),
            }
        }
        Err(error) => match format {
            OutputFormat::Human => write_data_stderr(&format!(
                "SEL-GPU001 GPU probe unavailable or failed: {}\n",
                error.message
            )),
            OutputFormat::Json => write_data_stderr(&format!(
                "{}\n",
                json::encode(&json::JsonValue::Object(BTreeMap::from([
                    (
                        "code".to_owned(),
                        json::JsonValue::String("SEL-GPU001".to_owned()),
                    ),
                    ("message".to_owned(), json::JsonValue::String(error.message),),
                ])))
            )),
        },
    }
}

fn adapter_json(adapter: &gpu::AdapterInfo) -> json::JsonValue {
    json::JsonValue::Object(BTreeMap::from([
        (
            "name".to_owned(),
            json::JsonValue::String(adapter.name.clone()),
        ),
        (
            "vendor".to_owned(),
            json::JsonValue::Number(adapter.vendor.to_string()),
        ),
        (
            "device".to_owned(),
            json::JsonValue::Number(adapter.device.to_string()),
        ),
        (
            "device_type".to_owned(),
            json::JsonValue::String(adapter.device_type.clone()),
        ),
        (
            "backend".to_owned(),
            json::JsonValue::String(adapter.backend.clone()),
        ),
        (
            "driver".to_owned(),
            json::JsonValue::String(adapter.driver.clone()),
        ),
        (
            "driver_info".to_owned(),
            json::JsonValue::String(adapter.driver_info.clone()),
        ),
    ]))
}

fn probe_json(report: &gpu::ProbeReport) -> BTreeMap<String, json::JsonValue> {
    BTreeMap::from([
        ("schema".to_owned(), json::JsonValue::Number("1".to_owned())),
        (
            "event".to_owned(),
            json::JsonValue::String("gpu.probe".to_owned()),
        ),
        ("adapter".to_owned(), adapter_json(&report.adapter)),
        (
            "elements".to_owned(),
            json::JsonValue::Number(report.elements.to_string()),
        ),
        (
            "max_abs_error".to_owned(),
            json::JsonValue::Number(format!("{:.9e}", report.max_abs_error)),
        ),
        (
            "result".to_owned(),
            json::JsonValue::Array(
                report
                    .result
                    .iter()
                    .map(|value| json::JsonValue::Number(format!("{value:.9e}")))
                    .collect(),
            ),
        ),
    ])
}

fn run_path(path: PathBuf, options: Options) -> u8 {
    if path.is_dir() {
        return run_project_path(&path, options);
    }
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return run_json_data_path(&path, options);
    }
    let label = display_argument(path.as_os_str());
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return write_io_diagnostic(&format!(
                "SEL-C003 failed to read source `{label}`: {error}\n"
            ));
        }
    };
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => {
            let offset = error.utf8_error().valid_up_to();
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-L001",
                format!("source is not valid UTF-8 at byte offset {offset}"),
                diagnostic::Span::new(offset, offset.saturating_add(1)),
            );
            return write_diagnostics(&path, "", &[diagnostic], options);
        }
    };

    let parsed = parser::parse(&source);
    if !parsed.diagnostics.is_empty() {
        return write_diagnostics(&path, &source, &parsed.diagnostics, options);
    }
    let Some(program) = parsed.program else {
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-C004",
            "parser returned no program without diagnostics",
            diagnostic::Span::new(0, 0),
        );
        return write_diagnostics(&path, &source, &[diagnostic], options);
    };
    let semantic_diagnostics = semantic::check(&program);
    if !semantic_diagnostics.is_empty() {
        return write_diagnostics(&path, &source, &semantic_diagnostics, options);
    }

    let base_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let mut event_sink = CliEventSink::new(options.log);
    let execution =
        runtime::execute_with_context_and_events(&program, base_dir, 128, 4, &mut event_sink);
    if let Some(error) = event_sink.take_failure() {
        return write_io_error("event log", error);
    }
    match execution {
        Ok(output) => write_execution_output(&path, &source, &program, output, options),
        Err(error) => {
            let diagnostic = error.diagnostic();
            if options.format == OutputFormat::Human {
                let mut rendered = diagnostic::render(&path, &source, &diagnostic);
                if !error.stack.is_empty() {
                    rendered.push_str(&format!(
                        "{}: stack {}\n",
                        path.display(),
                        error.stack.join(" -> ")
                    ));
                }
                write_data_stderr(&rendered)
            } else {
                write_diagnostics(&path, &source, &[diagnostic], options)
            }
        }
    }
}

fn check_json_data_path(path: &std::path::Path, options: Options) -> u8 {
    let label = display_argument(path.as_os_str());
    if let Err(error) = std::fs::read(path) {
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-C003",
            format!("failed to read source `{label}`: {error}"),
            diagnostic::Span::new(0, 0),
        );
        return write_diagnostics(path, "", &[diagnostic], options);
    }
    if options.quiet {
        EXIT_OK
    } else {
        write_stdout(&format!("checked {label}\n"))
    }
}

fn run_json_data_path(path: &std::path::Path, options: Options) -> u8 {
    let label = display_argument(path.as_os_str());
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return write_io_diagnostic(&format!(
                "SEL-C003 failed to read source `{label}`: {error}\n"
            ));
        }
    };
    match json::parse(&source, 128, 4) {
        Ok(value) => write_stdout(&format!("{}\n", json::encode(&value))),
        Err(error) => {
            let diagnostic = diagnostic::Diagnostic::new(
                error.code,
                error.message,
                diagnostic::Span::new(0, source.len()),
            );
            write_diagnostics(path, &source, &[diagnostic], options)
        }
    }
}

fn check_project_path(path: &std::path::Path, options: Options) -> u8 {
    let loaded = match project::load(path) {
        Ok(loaded) => loaded,
        Err(diagnostic) => return write_diagnostics(path, "", &[diagnostic], options),
    };
    let semantic_diagnostics = semantic::check(&loaded.program);
    if semantic_diagnostics.is_empty() {
        if options.quiet {
            EXIT_OK
        } else {
            write_stdout(&format!("checked {}\n", loaded.label))
        }
    } else {
        write_diagnostics(path, &loaded.source, &semantic_diagnostics, options)
    }
}

fn run_project_path(path: &std::path::Path, options: Options) -> u8 {
    run_project_path_with_diagnostic_path(path, path, options)
}

fn run_project_path_with_diagnostic_path(
    path: &std::path::Path,
    diagnostic_path: &std::path::Path,
    options: Options,
) -> u8 {
    let loaded = match project::load(path) {
        Ok(loaded) => loaded,
        Err(diagnostic) => return write_diagnostics(diagnostic_path, "", &[diagnostic], options),
    };
    let semantic_diagnostics = semantic::check(&loaded.program);
    if !semantic_diagnostics.is_empty() {
        return write_diagnostics(
            diagnostic_path,
            &loaded.source,
            &semantic_diagnostics,
            options,
        );
    }
    let mut event_sink = CliEventSink::new(options.log);
    let execution = runtime::execute_with_capabilities_and_events(
        &loaded.program,
        &loaded.base_dir,
        loaded.json_max_bytes,
        loaded.json_max_depth,
        &loaded.read_roots,
        &loaded.write_roots,
        &mut event_sink,
    );
    if let Some(error) = event_sink.take_failure() {
        return write_io_error("event log", error);
    }
    match execution {
        Ok(output) => write_execution_output(
            diagnostic_path,
            &loaded.source,
            &loaded.program,
            output,
            options,
        ),
        Err(error) => {
            let diagnostic = error.diagnostic();
            if options.format == OutputFormat::Human {
                let mut rendered = diagnostic::render(diagnostic_path, &loaded.source, &diagnostic);
                if !error.stack.is_empty() {
                    rendered.push_str(&format!(
                        "{}: stack {}\n",
                        diagnostic_path.display(),
                        error.stack.join(" -> ")
                    ));
                }
                write_data_stderr(&rendered)
            } else {
                write_diagnostics(diagnostic_path, &loaded.source, &[diagnostic], options)
            }
        }
    }
}

fn write_execution_output(
    path: &std::path::Path,
    source: &str,
    program: &parser::Program,
    output: runtime::ExecutionOutput,
    options: Options,
) -> u8 {
    let runtime::ExecutionOutput { value, stdout } = output;
    if let runtime::Value::Result { ok: false, .. } = &value {
        if !stdout.is_empty() {
            let status = write_stdout(&stdout);
            if status != EXIT_OK {
                return status;
            }
        }
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-R127",
            format!("program returned {}", value.render()),
            result_failure_span(program),
        );
        return write_diagnostics(path, source, &[diagnostic], options);
    }

    if options.format == OutputFormat::Json {
        let result = json::JsonValue::Object(BTreeMap::from([
            ("stdout".to_owned(), json::JsonValue::String(stdout)),
            ("value".to_owned(), runtime::value_to_json(&value)),
        ]));
        return write_stdout(&format!("{}\n", json::encode(&result)));
    }

    let mut rendered = stdout;
    if value != runtime::Value::Unit {
        rendered.push_str(&format!("{}\n", render_cli_value(program, value)));
    }
    if rendered.is_empty() {
        EXIT_OK
    } else {
        write_stdout(&rendered)
    }
}

fn result_failure_span(program: &parser::Program) -> diagnostic::Span {
    program
        .items
        .iter()
        .find_map(|item| match item {
            parser::Item::Function(function) if function.name == "main" => {
                first_control_flow_span(&function.body).or(Some(function.span))
            }
            _ => None,
        })
        .unwrap_or(program.span)
}

fn first_control_flow_span(statements: &[parser::Statement]) -> Option<diagnostic::Span> {
    for statement in statements {
        let span = match statement {
            parser::Statement::Return {
                value: Some(_),
                span,
            }
            | parser::Statement::Require { span, .. } => Some(*span),
            parser::Statement::If {
                branches,
                otherwise,
                ..
            } => branches
                .iter()
                .find_map(|branch| first_control_flow_span(&branch.body))
                .or_else(|| otherwise.as_deref().and_then(first_control_flow_span)),
            parser::Statement::Match(statement) => statement
                .cases
                .iter()
                .find_map(|case| first_control_flow_span(&case.body)),
            parser::Statement::For { body, .. } => first_control_flow_span(body),
            _ => None,
        };
        if span.is_some() {
            return span;
        }
    }
    None
}

fn build_path(path: PathBuf, profile: String, output: PathBuf) -> u8 {
    let loaded = match project::load(&path) {
        Ok(loaded) => loaded,
        Err(diagnostic) => return write_diagnostics(&path, "", &[diagnostic], Options::default()),
    };
    let semantic_diagnostics = semantic::check(&loaded.program);
    if !semantic_diagnostics.is_empty() {
        return write_diagnostics(
            &path,
            &loaded.source,
            &semantic_diagnostics,
            Options::default(),
        );
    }
    let bytes = match project::encode_bundle(&path, &profile) {
        Ok(bytes) => bytes,
        Err(diagnostic) => return write_diagnostics(&path, "", &[diagnostic], Options::default()),
    };
    if let Err(error) = fs::create_dir_all(&output) {
        return write_io_diagnostic(&format!(
            "SEL-C003 failed to create build output `{}`: {error}\n",
            display_argument(output.as_os_str())
        ));
    }
    let artifact_path = output.join("seloryn.selyapp");
    if let Err(error) = fs::write(&artifact_path, &bytes) {
        return write_io_diagnostic(&format!(
            "SEL-C003 failed to write build artifact `{}`: {error}\n",
            display_argument(artifact_path.as_os_str())
        ));
    }
    let mut result = BTreeMap::new();
    result.insert("absolute_paths".to_owned(), json::JsonValue::Bool(false));
    result.insert(
        "artifact".to_owned(),
        json::JsonValue::String(".selyapp".to_owned()),
    );
    result.insert(
        "bytes".to_owned(),
        json::JsonValue::Number(bytes.len().to_string()),
    );
    result.insert("deterministic".to_owned(), json::JsonValue::Bool(true));
    result.insert("profile".to_owned(), json::JsonValue::String(profile));
    result.insert(
        "same_inputs_same_hash".to_owned(),
        json::JsonValue::Bool(true),
    );
    write_stdout(&format!(
        "{}\n",
        json::encode(&json::JsonValue::Object(result))
    ))
}

fn exec_path(path: PathBuf) -> u8 {
    let artifact = match find_artifact(&path) {
        Ok(artifact) => artifact,
        Err(diagnostic) => return write_diagnostics(&path, "", &[diagnostic], Options::default()),
    };
    let bytes = match fs::read(&artifact) {
        Ok(bytes) => bytes,
        Err(error) => {
            return write_io_diagnostic(&format!(
                "SEL-C003 failed to read bundle `{}`: {error}\n",
                display_argument(artifact.as_os_str())
            ));
        }
    };
    let bundle = match project::decode_bundle(&bytes) {
        Ok(bundle) => bundle,
        Err(diagnostic) => {
            return write_diagnostics(&artifact, "", &[diagnostic], Options::default());
        }
    };
    let temporary = match create_bundle_directory() {
        Ok(path) => path,
        Err(diagnostic) => {
            return write_diagnostics(&artifact, "", &[diagnostic], Options::default());
        }
    };
    let result = match materialize_bundle(&temporary, &bundle) {
        Ok(()) => run_project_path_with_diagnostic_path(&temporary, &artifact, Options::default()),
        Err(diagnostic) => write_diagnostics(&artifact, "", &[diagnostic], Options::default()),
    };
    let _ = fs::remove_dir_all(&temporary);
    result
}

fn find_artifact(path: &std::path::Path) -> Result<PathBuf, diagnostic::Diagnostic> {
    if path.is_file() {
        if path
            .extension()
            .is_some_and(|extension| extension == "selyapp")
        {
            return Ok(path.to_path_buf());
        }
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "exec expects a `.selyapp` bundle or a directory containing one",
            diagnostic::Span::new(0, 0),
        ));
    }
    if !path.is_dir() {
        return Err(diagnostic::Diagnostic::new(
            "SEL-C003",
            "bundle path is not a file or directory",
            diagnostic::Span::new(0, 0),
        ));
    }
    let mut artifacts = fs::read_dir(path)
        .map_err(|_| {
            diagnostic::Diagnostic::new(
                "SEL-C003",
                "bundle directory is unreadable",
                diagnostic::Span::new(0, 0),
            )
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate.is_file()
                && candidate
                    .extension()
                    .is_some_and(|extension| extension == "selyapp")
        })
        .collect::<Vec<_>>();
    artifacts.sort();
    match artifacts.as_slice() {
        [artifact] => Ok(artifact.clone()),
        [] => Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle directory contains no `.selyapp` artifact",
            diagnostic::Span::new(0, 0),
        )),
        _ => Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle directory contains more than one `.selyapp` artifact",
            diagnostic::Span::new(0, 0),
        )),
    }
}

fn create_bundle_directory() -> Result<PathBuf, diagnostic::Diagnostic> {
    let base = env::temp_dir();
    for attempt in 0..1000_u32 {
        let candidate = base.join(format!("seloryn-exec-{}-{attempt}", std::process::id()));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(diagnostic::Diagnostic::new(
                    "SEL-C003",
                    format!("could not create temporary execution directory: {error}"),
                    diagnostic::Span::new(0, 0),
                ));
            }
        }
    }
    Err(diagnostic::Diagnostic::new(
        "SEL-C003",
        "could not allocate a unique temporary execution directory",
        diagnostic::Span::new(0, 0),
    ))
}

fn materialize_bundle(
    root: &std::path::Path,
    bundle: &project::Bundle,
) -> Result<(), diagnostic::Diagnostic> {
    for file in &bundle.files {
        let relative = std::path::Path::new(&file.path);
        if relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir | std::path::Component::Prefix(_)
                )
            })
        {
            return Err(diagnostic::Diagnostic::new(
                "SEL-B001",
                "bundle file path escapes the temporary execution directory",
                diagnostic::Span::new(0, 0),
            ));
        }
        let destination = root.join(relative);
        if !destination.starts_with(root) {
            return Err(diagnostic::Diagnostic::new(
                "SEL-B001",
                "bundle file path escapes the temporary execution directory",
                diagnostic::Span::new(0, 0),
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                diagnostic::Diagnostic::new(
                    "SEL-C003",
                    format!("could not materialize bundle directory: {error}"),
                    diagnostic::Span::new(0, 0),
                )
            })?;
        }
        fs::write(destination, &file.bytes).map_err(|error| {
            diagnostic::Diagnostic::new(
                "SEL-C003",
                format!("could not materialize bundle source: {error}"),
                diagnostic::Span::new(0, 0),
            )
        })?;
    }
    Ok(())
}

fn render_cli_value(program: &parser::Program, value: runtime::Value) -> String {
    let unwrap = program.items.iter().any(|item| {
        matches!(
            item,
            parser::Item::Directive(directive)
                if directive.keyword == "from"
                    && directive
                        .text
                        .split_whitespace()
                        .next()
                        .is_some_and(|module| matches!(module, "json" | "models"))
        )
    });
    if unwrap && let runtime::Value::Result { ok: true, value } = value {
        return match *value {
            runtime::Value::Text(value) => value,
            value => value.render(),
        };
    }
    value.render()
}

fn write_stdout(message: &str) -> u8 {
    match io::stdout().write_all(message.as_bytes()) {
        Ok(()) => EXIT_OK,
        Err(error) => write_io_error("stdout", error),
    }
}

fn write_stderr(message: &str) -> u8 {
    match io::stderr().write_all(message.as_bytes()) {
        Ok(()) => EXIT_USAGE,
        Err(error) => write_io_error("stderr", error),
    }
}

fn write_data_stderr(message: &str) -> u8 {
    match io::stderr().write_all(message.as_bytes()) {
        Ok(()) => EXIT_DATA,
        Err(error) => write_io_error("stderr", error),
    }
}

fn write_diagnostics(
    path: &std::path::Path,
    source: &str,
    diagnostics: &[diagnostic::Diagnostic],
    options: Options,
) -> u8 {
    let _ = options.color;
    let rendered = match options.format {
        OutputFormat::Human => diagnostics
            .iter()
            .map(|diagnostic| diagnostic::render(path, source, diagnostic))
            .collect::<String>(),
        OutputFormat::Json => diagnostic::render_json(path, source, diagnostics),
    };
    write_data_stderr(&rendered)
}

fn write_io_diagnostic(message: &str) -> u8 {
    match io::stderr().write_all(message.as_bytes()) {
        Ok(()) => EXIT_IO,
        Err(error) => write_io_error("stderr", error),
    }
}

fn write_io_error(stream: &str, error: io::Error) -> u8 {
    let _ =
        io::stderr().write_all(format!("SEL-C002 failed to write {stream}: {error}\n").as_bytes());
    EXIT_IO
}

#[cfg(test)]
mod tests {
    use super::{Action, classify, display_argument};
    use std::ffi::OsString;

    #[test]
    fn no_arguments_open_help() {
        assert!(matches!(classify(&[]), Action::Help));
    }

    #[test]
    fn help_alias_is_supported() {
        assert!(matches!(classify(&[OsString::from("-h")]), Action::Help));
    }

    #[test]
    fn version_is_a_single_supported_argument() {
        assert!(matches!(
            classify(&[OsString::from("--version")]),
            Action::Version
        ));
        assert!(matches!(
            classify(&[OsString::from("--version"), OsString::from("extra")]),
            Action::Unsupported(_)
        ));
    }

    #[test]
    fn check_requires_exactly_one_source_path() {
        assert!(matches!(
            classify(&[OsString::from("check"), OsString::from("program.sely")]),
            Action::Check { .. }
        ));
        assert!(matches!(
            classify(&[OsString::from("check")]),
            Action::Unsupported(_)
        ));
        assert!(matches!(
            classify(&[OsString::from("run"), OsString::from("program.sely")]),
            Action::Run { .. }
        ));
        assert!(matches!(
            classify(&[
                OsString::from("build"),
                OsString::from("project"),
                OsString::from("--profile"),
                OsString::from("debug"),
                OsString::from("--output"),
                OsString::from("artifact"),
            ]),
            Action::Build { .. }
        ));
        assert!(matches!(
            classify(&[OsString::from("exec"), OsString::from("app.selyapp")]),
            Action::Exec { .. }
        ));
    }

    #[test]
    fn device_commands_accept_only_the_format_option() {
        assert!(matches!(
            classify(&[OsString::from("devices"), OsString::from("--format=json")]),
            Action::Devices { .. }
        ));
        assert!(matches!(
            classify(&[
                OsString::from("gpu-probe"),
                OsString::from("--format"),
                OsString::from("human")
            ]),
            Action::GpuProbe { .. }
        ));
        assert!(matches!(
            classify(&[OsString::from("gpu-probe"), OsString::from("--log=json")]),
            Action::Unsupported(_)
        ));
    }

    #[test]
    fn unsupported_unicode_argument_remains_visible() {
        let action = classify(&[OsString::from("入口")]);
        match action {
            Action::Unsupported(message) => assert_eq!(message, "入口"),
            Action::Help | Action::Version | Action::Check { .. } => {
                panic!("expected unsupported action")
            }
            Action::Devices { .. }
            | Action::GpuProbe { .. }
            | Action::Run { .. }
            | Action::Build { .. }
            | Action::Exec { .. } => {
                panic!("expected unsupported action")
            }
        }
    }

    #[test]
    fn control_characters_are_escaped_in_diagnostics() {
        assert_eq!(
            display_argument(OsString::from("a\n\u{1b}b").as_os_str()),
            r"a\n\u{1b}b"
        );
    }
}
