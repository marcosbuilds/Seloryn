mod diagnostic;
mod lexer;
mod parser;
mod semantic;

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

const HELP_TEXT: &str = "Seloryn — native CLI bootstrap\n\nUsage:\n  seloryn --help\n  seloryn --version\n  seloryn check <file.sely>\n\n`check` validates UTF-8, layout, syntax and the E3 semantic core.\nThe interpreter, standard library and installable release\nare not available yet.\n";

enum Action {
    Help,
    Version,
    Check(OsString),
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
        Action::Check(path) => check_path(PathBuf::from(path)),
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
        [command, path] if command == "check" => Action::Check(path.clone()),
        _ => Action::Unsupported(
            args.iter()
                .map(|argument| display_argument(argument))
                .collect::<Vec<_>>()
                .join(" "),
        ),
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

fn check_path(path: PathBuf) -> u8 {
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
            return write_diagnostics(&diagnostic::render_at_start(&path, &diagnostic));
        }
    };

    let parsed = parser::parse(&source);
    if parsed.diagnostics.is_empty() {
        let program = parsed
            .program
            .expect("a diagnostic-free parse has a program");
        let semantic_diagnostics = semantic::check(&program);
        if semantic_diagnostics.is_empty() {
            return write_stdout(&format!("checked {label} (syntax + semantics)\n"));
        }
        let rendered = semantic_diagnostics
            .iter()
            .map(|diagnostic| diagnostic::render(&path, &source, diagnostic))
            .collect::<String>();
        return write_diagnostics(&rendered);
    }
    let rendered = parsed
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic::render(&path, &source, diagnostic))
        .collect::<String>();
    write_diagnostics(&rendered)
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

fn write_diagnostics(message: &str) -> u8 {
    match io::stderr().write_all(message.as_bytes()) {
        Ok(()) => EXIT_DATA,
        Err(error) => write_io_error("stderr", error),
    }
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
            Action::Check(_)
        ));
        assert!(matches!(
            classify(&[OsString::from("check")]),
            Action::Unsupported(_)
        ));
    }

    #[test]
    fn unsupported_unicode_argument_remains_visible() {
        let action = classify(&[OsString::from("入口")]);
        match action {
            Action::Unsupported(message) => assert_eq!(message, "入口"),
            Action::Help | Action::Version | Action::Check(_) => {
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
