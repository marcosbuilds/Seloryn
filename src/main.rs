use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

const LANGUAGE_VERSION: &str = "0.00";
const DEVELOPMENT_BUILD_ID: &str = "local-development";
const EXIT_OK: u8 = 0;
const EXIT_USAGE: u8 = 64;
const EXIT_IO: u8 = 74;

const HELP_TEXT: &str = "Seloryn — native CLI bootstrap\n\nUsage:\n  seloryn --help\n  seloryn --version\n\nThis build exposes the CLI bootstrap only. The language compiler,\ninterpreter, standard library and installable release are not available yet.\n";

enum Action {
    Help,
    Version,
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
            "seloryn {LANGUAGE_VERSION} build={DEVELOPMENT_BUILD_ID} status=design-only\n"
        )),
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
        _ => Action::Unsupported(
            args.iter()
                .map(display_argument)
                .collect::<Vec<_>>()
                .join(" "),
        ),
    }
}

fn display_argument(argument: &OsString) -> String {
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
    fn unsupported_unicode_argument_remains_visible() {
        let action = classify(&[OsString::from("入口")]);
        match action {
            Action::Unsupported(message) => assert_eq!(message, "入口"),
            Action::Help | Action::Version => panic!("expected unsupported action"),
        }
    }

    #[test]
    fn control_characters_are_escaped_in_diagnostics() {
        assert_eq!(
            display_argument(&OsString::from("a\n\u{1b}b")),
            r"a\n\u{1b}b"
        );
    }
}
