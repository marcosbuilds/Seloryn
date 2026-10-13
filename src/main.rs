mod builtins;
mod diagnostic;
mod lexer;
mod parser;
mod runtime;
mod semantic;
mod tensor;

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

const HELP_TEXT: &str = "Seloryn — native CLI bootstrap\n\nUsage:\n  seloryn --help\n  seloryn --version\n  seloryn check <file.sely> [--format=human|json] [--quiet]\n  seloryn run <file.sely> [--format=human|json]\n\n`check` validates UTF-8, layout, syntax and the E3 semantic core.\n`run` validates the same front end, then executes the pure E4 scalar and N1/N2 CPU tensor subset.\nThe standard library and installable release are not available yet.\n";

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
struct Options {
    format: OutputFormat,
    color: ColorMode,
    quiet: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            format: OutputFormat::Human,
            color: ColorMode::Auto,
            quiet: false,
        }
    }
}

enum Action {
    Help,
    Version,
    Check { path: OsString, options: Options },
    Run { path: OsString, options: Options },
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
        _ => unsupported_arguments(args),
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
        } else if argument == "--format" || argument == "--color" {
            let Some(value) = args.get(index + 1) else {
                return unsupported_arguments(args);
            };
            if argument == "--format" {
                let Some(format) = parse_format(value) else {
                    return unsupported_arguments(args);
                };
                options.format = format;
            } else {
                let Some(color) = parse_color(value) else {
                    return unsupported_arguments(args);
                };
                options.color = color;
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
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
        {
            return unsupported_arguments(args);
        } else if path.replace(argument.clone()).is_some() {
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

    match runtime::execute_with_output(&program) {
        Ok(output) => {
            let mut rendered = output.stdout;
            if output.value != runtime::Value::Unit {
                rendered.push_str(&format!("{}\n", output.value.render()));
            }
            if rendered.is_empty() {
                EXIT_OK
            } else {
                write_stdout(&rendered)
            }
        }
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
    }

    #[test]
    fn unsupported_unicode_argument_remains_visible() {
        let action = classify(&[OsString::from("入口")]);
        match action {
            Action::Unsupported(message) => assert_eq!(message, "入口"),
            Action::Help | Action::Version | Action::Check { .. } => {
                panic!("expected unsupported action")
            }
            Action::Run { .. } => panic!("expected unsupported action"),
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
