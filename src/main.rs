#![allow(clippy::result_large_err)]

mod autograd;
mod builtins;
mod diagnostic;
mod ir;
mod json;
mod lexer;
mod parser;
mod project;
mod runtime;
mod semantic;
mod tensor;

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

const LANGUAGE_VERSION: &str = "0.00";
const DEVELOPMENT_BUILD_ID: &str = "local-development";
const EXIT_OK: u8 = 0;
const EXIT_USAGE: u8 = 64;
const EXIT_DATA: u8 = 65;
const EXIT_IO: u8 = 74;

const HELP_TEXT: &str = "Seloryn — native CLI bootstrap\n\nUsage:\n  seloryn --help\n  seloryn --version\n  seloryn check <file.sely> [--format=human|json] [--quiet]\n  seloryn run <file.sely> [--format=human|json] [--log=off|human|json]\n  seloryn build <project> --profile debug|release --output <directory>\n  seloryn exec <bundle.selyapp|directory>\n\n`check` validates UTF-8, layout, syntax and the E3 semantic core.\n`run` validates the same front end, then executes the pure E4 scalar and N1/N2 CPU tensor subset.\n`--log` is off by default; when enabled, narrow training progress is streamed to stderr.\n`build` creates a deterministic experimental `.selyapp` bundle; `exec` runs one.\nNo installer or supported release is available yet.\n";

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
            "seloryn {LANGUAGE_VERSION} build={DEVELOPMENT_BUILD_ID} status=experimental-implemented\n"
        )),
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
        [command, rest @ ..] if command == "check" => parse_command("check", rest),
        [command, rest @ ..] if command == "run" => parse_command("run", rest),
        [command, rest @ ..] if command == "build" => parse_build(rest),
        [command, rest @ ..] if command == "exec" => parse_exec(rest),
        _ => unsupported_arguments(args),
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
        if let Err(error) = io::stderr().write_all(format!("{line}\n").as_bytes()) {
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
        } => format!(
            "seloryn training.start model={} train={} test={} train_samples={} test_samples={} steps={} seed_metadata={}",
            model,
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
            loss,
        } => format!(
            "seloryn training.step model={} step={}/{} loss={loss:.8}",
            model, step, steps
        ),
        runtime::RuntimeEvent::Completed {
            model,
            steps,
            seed,
            initial_loss,
            final_loss,
            holdout_mae_observed,
        } => format!(
            "seloryn training.complete model={} steps={} initial_loss={initial_loss:.8} final_loss={final_loss:.8} holdout_mae_observed={holdout_mae_observed:.8} seed_metadata={}",
            model, steps, seed
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
        } => {
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.start".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
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
            loss,
        } => {
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
                "loss".to_owned(),
                json::JsonValue::Number(format!("{loss:.8}")),
            );
        }
        runtime::RuntimeEvent::Completed {
            model,
            steps,
            seed,
            initial_loss,
            final_loss,
            holdout_mae_observed,
        } => {
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.complete".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
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
                "initial_loss".to_owned(),
                json::JsonValue::Number(format!("{initial_loss:.8}")),
            );
            object.insert(
                "final_loss".to_owned(),
                json::JsonValue::Number(format!("{final_loss:.8}")),
            );
            object.insert(
                "holdout_mae_observed".to_owned(),
                json::JsonValue::Number(format!("{holdout_mae_observed:.8}")),
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
    if parsed.diagnostics.is_empty() {
        let Some(program) = parsed.program else {
            return write_stderr("SEL-C004 parser returned no program without diagnostics\n");
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
        return write_stderr("SEL-C004 parser returned no program without diagnostics\n");
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
        return write_io_diagnostic(&format!(
            "SEL-C003 failed to read source `{label}`: {error}\n"
        ));
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
    fn unsupported_unicode_argument_remains_visible() {
        let action = classify(&[OsString::from("入口")]);
        match action {
            Action::Unsupported(message) => assert_eq!(message, "入口"),
            Action::Help | Action::Version | Action::Check { .. } => {
                panic!("expected unsupported action")
            }
            Action::Run { .. } | Action::Build { .. } | Action::Exec { .. } => {
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
