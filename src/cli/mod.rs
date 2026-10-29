#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    mpsc::{SyncSender, sync_channel},
};
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::compute::gpu;
use crate::data::sha256_hex;
use crate::jobs as job;
use crate::language::{analysis, parser, semantic};
use crate::project;
use crate::protocols::lsp;
use crate::{diagnostic, json, runtime};

mod output;
mod workspace;

use output::{CliEventSink, RunIdentity, display_argument, is_receipt_log};

const CANCELLATION_ERROR_CODE: &str = "SEL-CANCELLED";
const CANCELLATION_MESSAGE: &str = "execution cancellation requested";
const EXIT_CANCELLED: u8 = 130;

fn install_cancellation_handler() -> io::Result<()> {
    runtime::control::install()
}

pub(crate) fn cancellation_requested() -> bool {
    runtime::control::requested()
}

pub(crate) const LANGUAGE_VERSION: &str = "0.12";
const PACKAGE_VERSION: &str = env!("CARGO_PKG_VERSION");
const DEVELOPMENT_BUILD_ID: &str = "local-development";
pub(crate) const EXIT_OK: u8 = 0;
const EXIT_USAGE: u8 = 64;
pub(crate) const EXIT_DATA: u8 = 65;
pub(crate) const EXIT_IO: u8 = 74;

const HELP_TEXT: &str = "Seloryn — native CLI\n\nUsage:\n  seloryn --help\n  seloryn --version\n  seloryn devices [--format=human|json]\n  seloryn gpu-probe [--format=human|json]\n  seloryn doctor [project] [--format=human|json]\n  seloryn init <directory> [--template=minimal|tabular-mlp] [--name=<name>] [--format=human|json]\n  seloryn check <file.sely|project> [--format=human|json] [--quiet]\n  seloryn inspect <file.sely|project> [--format=human|json]\n  seloryn run <file.sely|project> [--format=human|json] [--log=off|human|json|json2|json2-signed]\n  seloryn build <project> --profile debug|release --output <directory>\n  seloryn exec <bundle.selyapp|directory>\n  seloryn lsp --stdio\n\n`check` validates UTF-8, layout, syntax and the supported semantic core.\n`run` executes bounded scalar, CPU tensor, reverse-mode, Seloryn-authored, CPU MLP and bounded GPU linear training profiles.\n`run --format=json` writes a stable `{stdout,value}` result envelope to stdout on success.\n`from json import read_stdin` accepts one bounded JSON document from stdin.\n`--log` is off by default; `human` and `json` preserve the v1 training stream, `json2` adds an identity-bearing hash-chained receipt, and `json2-signed` adds HMAC-SHA256 authentication using `SELORYN_RECEIPT_KEY`.\n`devices` enumerates the compiled hardware adapters without selecting one for training.\n`gpu-probe` submits a bounded compute shader and verifies readback on a physical non-fallback adapter.\n`doctor` performs read-only project validation and reports production blockers; it never executes project code.\n`init` creates a validated project skeleton without overwriting an existing path.\n`build` creates a deterministic `.selyapp` bundle; `exec` runs one.\n`inspect` emits compact project/source symbols, imports and diagnostics in the compatible `seloryn.analysis.v1` envelope; `--format=json` is intended for tools and agents.\n`lsp --stdio` provides bounded full-document diagnostics, completion, definition and document symbols backed by the parser.\nGPU training is explicit through `from training import gpu` (the compatible `train_linear_gpu` name remains accepted) in a `.sely` project; it requires a physical adapter and never silently falls back to CPU. Distributed execution remains unavailable.\n";

const USAGE_HELP: &str = "run `seloryn --help` for the complete command list";
const COLOR_HELP_TEXT: &str = "`--color=auto|always|never` controls ANSI colors for human diagnostics; JSON is always plain.\n";
const CHECK_USAGE: &str =
    "seloryn check <file.sely|project> [--format=human|json] [--color=auto|always|never] [--quiet]";
const RUN_USAGE: &str = "seloryn run <file.sely|project> [--format=human|json] [--color=auto|always|never] [--log=off|human|json|json2|json2-signed]";
const INSPECT_USAGE: &str = "seloryn inspect <file.sely|project> [--format=human|json]";
const DOCTOR_USAGE: &str = "seloryn doctor [project] [--format=human|json]";
const INIT_USAGE: &str = "seloryn init <directory> [--template=minimal|tabular-mlp] [--name=<name>] [--format=human|json]";
const DEVICES_USAGE: &str = "seloryn devices|gpu-probe [--format=human|json]";
const BUILD_USAGE: &str = "seloryn build <project> --profile debug|release --output <directory>";
const EXEC_USAGE: &str = "seloryn exec <bundle.selyapp|directory>";
const LSP_USAGE: &str = "seloryn lsp --stdio";
const JOB_USAGE: &str = "seloryn job <create|status|list|transition|retry|claim|heartbeat|release|recover|preflight|run> ...";

const JOB_HELP_TEXT: &str = "Jobs:\n  seloryn job create <spec.json> [--id=<job-id>] [--root=<directory>] [--format=human|json]\n  seloryn job status <job-id> [--root=<directory>] [--format=human|json]\n  seloryn job list [--root=<directory>] [--format=human|json]\n  seloryn job transition <job-id> <status> [--root=<directory>] [--format=human|json]\n  seloryn job retry <job-id> [--root=<directory>] [--format=human|json]\n  seloryn job claim <job-id> [--owner=<owner>] [--lease-id=<id>] [--ttl-seconds=<n>] [--root=<directory>] [--format=human|json]\n  seloryn job heartbeat <job-id> --lease-id=<id> [--ttl-seconds=<n>] [--root=<directory>] [--format=human|json]\n  seloryn job release <job-id> --lease-id=<id> [--root=<directory>] [--format=human|json]\n  seloryn job recover <job-id> [--root=<directory>] [--format=human|json]\n  seloryn job preflight <job-id> [--root=<directory>] [--format=human|json]\n";

const WORKER_HELP_TEXT: &str = "  seloryn job run <job-id> [--owner=<owner>] [--binary=<seloryn>] [--root=<directory>] [--format=human|json]\n  job run performs bounded local execution only after preflight and lease claim; it never invokes a shell or an arbitrary executable.\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
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
    Json2,
    Json2Signed,
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
    Doctor {
        path: OsString,
        format: OutputFormat,
    },
    Init {
        path: OsString,
        template: workspace::InitTemplate,
        name: Option<OsString>,
        format: OutputFormat,
    },
    Check {
        path: OsString,
        options: Options,
    },
    Inspect {
        path: OsString,
        format: OutputFormat,
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
    JobHelp,
    Job(JobAction),
    Lsp,
    Unsupported(String),
}

enum JobAction {
    Create {
        spec: OsString,
        id: Option<OsString>,
        root: OsString,
        format: OutputFormat,
    },
    Status {
        id: OsString,
        root: OsString,
        format: OutputFormat,
    },
    List {
        root: OsString,
        format: OutputFormat,
    },
    Transition {
        id: OsString,
        status: OsString,
        root: OsString,
        format: OutputFormat,
    },
    Retry {
        id: OsString,
        root: OsString,
        format: OutputFormat,
    },
    Claim {
        id: OsString,
        owner: Option<OsString>,
        lease_id: Option<OsString>,
        ttl_seconds: u64,
        root: OsString,
        format: OutputFormat,
    },
    Heartbeat {
        id: OsString,
        lease_id: OsString,
        ttl_seconds: u64,
        root: OsString,
        format: OutputFormat,
    },
    Release {
        id: OsString,
        lease_id: OsString,
        root: OsString,
        format: OutputFormat,
    },
    Recover {
        id: OsString,
        root: OsString,
        format: OutputFormat,
    },
    Preflight {
        id: OsString,
        root: OsString,
        format: OutputFormat,
    },
    Run {
        id: OsString,
        owner: Option<OsString>,
        binary: Option<OsString>,
        root: OsString,
        format: OutputFormat,
    },
}

pub fn run(args: &[OsString]) -> u8 {
    match classify(args) {
        Action::Help => write_stdout(&format!(
            "{HELP_TEXT}{JOB_HELP_TEXT}{WORKER_HELP_TEXT}{COLOR_HELP_TEXT}Source-owned workflows can emit structured audit events through `from audit import emit`; JSON2 includes them in the same hash-chained receipt.\nCPU training remains the verified path for the MLP;\n"
        )),
        Action::Version => write_stdout(&format!(
            "seloryn {LANGUAGE_VERSION} package={PACKAGE_VERSION} build={DEVELOPMENT_BUILD_ID} status=verified-preview\n"
        )),
        Action::Devices { format } => devices(format),
        Action::GpuProbe { format } => gpu_probe(format),
        Action::Doctor { path, format } => workspace::doctor(&PathBuf::from(path), format),
        Action::Init {
            path,
            template,
            name,
            format,
        } => workspace::init(&PathBuf::from(path), template, name.as_deref(), format),
        Action::Check { path, options } => check_path(PathBuf::from(path), options),
        Action::Inspect { path, format } => inspect_path(PathBuf::from(path), format),
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
        Action::JobHelp => write_stdout(&format!("{JOB_HELP_TEXT}{WORKER_HELP_TEXT}")),
        Action::Job(action) => run_job(action),
        Action::Lsp => lsp::serve_stdio(),
        Action::Unsupported(command) => write_stderr(&format!("SEL-C001 {command}\n")),
    }
}

fn classify(args: &[OsString]) -> Action {
    match args {
        [] => Action::Help,
        [argument] if argument == "--help" || argument == "-h" => Action::Help,
        [argument] if argument == "--version" => Action::Version,
        [command, rest @ ..] if command == "devices" => parse_device_command("devices", rest),
        [command, rest @ ..] if command == "gpu-probe" => parse_device_command("gpu-probe", rest),
        [command, rest @ ..] if command == "doctor" => parse_doctor(rest),
        [command, rest @ ..] if command == "init" => parse_init(rest),
        [command, rest @ ..] if command == "check" => parse_command("check", rest),
        [command, rest @ ..] if command == "inspect" => parse_inspect(rest),
        [command, rest @ ..] if command == "run" => parse_command("run", rest),
        [command, rest @ ..] if command == "build" => parse_build(rest),
        [command, rest @ ..] if command == "exec" => parse_exec(rest),
        [command, rest @ ..] if command == "job" => parse_job(rest),
        [command, rest @ ..] if command == "lsp" => parse_lsp(rest),
        _ => usage_error(args, "seloryn --help"),
    }
}

fn parse_job(args: &[OsString]) -> Action {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        return Action::JobHelp;
    }
    let Some(command) = args.first().and_then(|value| value.to_str()) else {
        return usage_error(args, JOB_USAGE);
    };
    let mut positionals = Vec::new();
    let mut id = None;
    let mut owner = None;
    let mut lease_id = None;
    let mut ttl_seconds = None;
    let mut binary = None;
    let mut root = OsString::from(".");
    let mut format = OutputFormat::Human;
    let mut index = 1;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--id"
            || argument == "--owner"
            || argument == "--lease-id"
            || argument == "--ttl-seconds"
            || argument == "--binary"
            || argument == "--root"
            || argument == "--format"
        {
            let Some(value) = args.get(index + 1) else {
                return usage_error(args, JOB_USAGE);
            };
            if argument == "--id" {
                id = Some(value.clone());
            } else if argument == "--owner" {
                owner = Some(value.clone());
            } else if argument == "--lease-id" {
                lease_id = Some(value.clone());
            } else if argument == "--ttl-seconds" {
                let Some(parsed) = value.to_str().and_then(|value| value.parse::<u64>().ok())
                else {
                    return usage_error(args, JOB_USAGE);
                };
                ttl_seconds = Some(parsed);
            } else if argument == "--binary" {
                binary = Some(value.clone());
            } else if argument == "--root" {
                root = value.clone();
            } else {
                let Some(parsed) = parse_format(value) else {
                    return usage_error(args, JOB_USAGE);
                };
                format = parsed;
            }
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--id="))
        {
            id = Some(OsString::from(value));
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--owner="))
        {
            owner = Some(OsString::from(value));
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--lease-id="))
        {
            lease_id = Some(OsString::from(value));
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--ttl-seconds="))
        {
            let Some(parsed) = value.parse::<u64>().ok() else {
                return usage_error(args, JOB_USAGE);
            };
            ttl_seconds = Some(parsed);
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--binary="))
        {
            binary = Some(OsString::from(value));
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--root="))
        {
            root = OsString::from(value);
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(parsed) = parse_format_text(value) else {
                return usage_error(args, JOB_USAGE);
            };
            format = parsed;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
        {
            return usage_error(args, JOB_USAGE);
        } else {
            positionals.push(argument.clone());
        }
        index += 1;
    }
    if command != "run" && binary.is_some() {
        return usage_error(args, JOB_USAGE);
    }
    match command {
        "create"
            if positionals.len() == 1
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Create {
                spec: positionals.remove(0),
                id,
                root,
                format,
            })
        }
        "status"
            if positionals.len() == 1
                && id.is_none()
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Status {
                id: positionals.remove(0),
                root,
                format,
            })
        }
        "list"
            if positionals.is_empty()
                && id.is_none()
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::List { root, format })
        }
        "transition"
            if positionals.len() == 2
                && id.is_none()
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Transition {
                id: positionals.remove(0),
                status: positionals.remove(0),
                root,
                format,
            })
        }
        "retry"
            if positionals.len() == 1
                && id.is_none()
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Retry {
                id: positionals.remove(0),
                root,
                format,
            })
        }
        "claim" if positionals.len() == 1 && id.is_none() => Action::Job(JobAction::Claim {
            id: positionals.remove(0),
            owner,
            lease_id,
            ttl_seconds: ttl_seconds.unwrap_or(60),
            root,
            format,
        }),
        "run"
            if positionals.len() == 1
                && id.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Run {
                id: positionals.remove(0),
                owner,
                binary,
                root,
                format,
            })
        }
        "heartbeat"
            if positionals.len() == 1 && id.is_none() && owner.is_none() && lease_id.is_some() =>
        {
            Action::Job(JobAction::Heartbeat {
                id: positionals.remove(0),
                lease_id: lease_id.expect("checked above"),
                ttl_seconds: ttl_seconds.unwrap_or(60),
                root,
                format,
            })
        }
        "release"
            if positionals.len() == 1
                && id.is_none()
                && owner.is_none()
                && lease_id.is_some()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Release {
                id: positionals.remove(0),
                lease_id: lease_id.expect("checked above"),
                root,
                format,
            })
        }
        "recover"
            if positionals.len() == 1
                && id.is_none()
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Recover {
                id: positionals.remove(0),
                root,
                format,
            })
        }
        "preflight"
            if positionals.len() == 1
                && id.is_none()
                && owner.is_none()
                && lease_id.is_none()
                && ttl_seconds.is_none() =>
        {
            Action::Job(JobAction::Preflight {
                id: positionals.remove(0),
                root,
                format,
            })
        }
        _ => usage_error(args, JOB_USAGE),
    }
}

fn parse_doctor(args: &[OsString]) -> Action {
    let mut path = None;
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--format" {
            let Some(value) = args.get(index + 1) else {
                return usage_error(args, DOCTOR_USAGE);
            };
            let Some(parsed) = parse_format(value) else {
                return usage_error(args, DOCTOR_USAGE);
            };
            format = parsed;
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(parsed) = parse_format_text(value) else {
                return usage_error(args, DOCTOR_USAGE);
            };
            format = parsed;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
            || path.replace(argument.clone()).is_some()
        {
            return usage_error(args, DOCTOR_USAGE);
        }
        index += 1;
    }
    Action::Doctor {
        path: path.unwrap_or_else(|| OsString::from(".")),
        format,
    }
}

fn parse_init(args: &[OsString]) -> Action {
    let mut path = None;
    let mut template = workspace::InitTemplate::Minimal;
    let mut name = None;
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--template" || argument == "--name" || argument == "--format" {
            let Some(value) = args.get(index + 1) else {
                return usage_error(args, INIT_USAGE);
            };
            if argument == "--template" {
                let Some(parsed) = workspace::parse_template(value) else {
                    return usage_error(args, INIT_USAGE);
                };
                template = parsed;
            } else if argument == "--name" {
                name = Some(value.clone());
            } else {
                let Some(parsed) = parse_format(value) else {
                    return usage_error(args, INIT_USAGE);
                };
                format = parsed;
            }
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--template="))
        {
            let Some(parsed) = workspace::parse_template_text(value) else {
                return usage_error(args, INIT_USAGE);
            };
            template = parsed;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--name="))
        {
            name = Some(OsString::from(value));
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(parsed) = parse_format_text(value) else {
                return usage_error(args, INIT_USAGE);
            };
            format = parsed;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
            || path.replace(argument.clone()).is_some()
        {
            return usage_error(args, INIT_USAGE);
        }
        index += 1;
    }
    let Some(path) = path else {
        return usage_error(args, INIT_USAGE);
    };
    Action::Init {
        path,
        template,
        name,
        format,
    }
}

fn parse_lsp(args: &[OsString]) -> Action {
    if args.len() == 1 && args[0] == "--stdio" {
        Action::Lsp
    } else {
        usage_error(args, LSP_USAGE)
    }
}

fn parse_device_command(command: &str, args: &[OsString]) -> Action {
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--format" {
            let Some(value) = args.get(index + 1) else {
                return usage_error(args, DEVICES_USAGE);
            };
            let Some(parsed) = parse_format(value) else {
                return usage_error(args, DEVICES_USAGE);
            };
            format = parsed;
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(parsed) = parse_format_text(value) else {
                return usage_error(args, DEVICES_USAGE);
            };
            format = parsed;
        } else {
            return usage_error(args, DEVICES_USAGE);
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
                return usage_error(args, BUILD_USAGE);
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
            return usage_error(args, BUILD_USAGE);
        }
        index += 1;
    }
    match (path, profile, output) {
        (Some(path), Some(profile), Some(output)) => Action::Build {
            path,
            profile,
            output,
        },
        _ => usage_error(args, BUILD_USAGE),
    }
}

fn parse_exec(args: &[OsString]) -> Action {
    if args.len() == 1 {
        Action::Exec {
            path: args[0].clone(),
        }
    } else {
        usage_error(args, EXEC_USAGE)
    }
}

fn parse_inspect(args: &[OsString]) -> Action {
    let mut path = None;
    let mut format = OutputFormat::Human;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--format" {
            let Some(value) = args.get(index + 1) else {
                return usage_error(args, INSPECT_USAGE);
            };
            let Some(parsed) = parse_format(value) else {
                return usage_error(args, INSPECT_USAGE);
            };
            format = parsed;
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(parsed) = parse_format_text(value) else {
                return usage_error(args, INSPECT_USAGE);
            };
            format = parsed;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
            || path.replace(argument.clone()).is_some()
        {
            return usage_error(args, INSPECT_USAGE);
        }
        index += 1;
    }
    let Some(path) = path else {
        return usage_error(args, INSPECT_USAGE);
    };
    Action::Inspect { path, format }
}

fn parse_command(command: &str, args: &[OsString]) -> Action {
    let usage = if command == "check" {
        CHECK_USAGE
    } else {
        RUN_USAGE
    };
    let mut options = Options::default();
    let mut path = None;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--quiet" {
            options.quiet = true;
        } else if argument == "--format" || argument == "--color" || argument == "--log" {
            let Some(value) = args.get(index + 1) else {
                return usage_error(args, usage);
            };
            if argument == "--format" {
                let Some(format) = parse_format(value) else {
                    return usage_error(args, usage);
                };
                options.format = format;
            } else if argument == "--color" {
                let Some(color) = parse_color(value) else {
                    return usage_error(args, usage);
                };
                options.color = color;
            } else {
                let Some(log) = parse_log(value) else {
                    return usage_error(args, usage);
                };
                options.log = log;
            }
            index += 1;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--format="))
        {
            let Some(format) = parse_format_text(value) else {
                return usage_error(args, usage);
            };
            options.format = format;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--color="))
        {
            let Some(color) = parse_color_text(value) else {
                return usage_error(args, usage);
            };
            options.color = color;
        } else if let Some(value) = argument
            .to_str()
            .and_then(|value| value.strip_prefix("--log="))
        {
            let Some(log) = parse_log_text(value) else {
                return usage_error(args, usage);
            };
            options.log = log;
        } else if argument
            .to_str()
            .is_some_and(|value| value.starts_with('-'))
            || path.replace(argument.clone()).is_some()
        {
            return usage_error(args, usage);
        }
        index += 1;
    }
    let Some(path) = path else {
        return usage_error(args, usage);
    };
    match command {
        "check" => Action::Check { path, options },
        "run" => Action::Run { path, options },
        _ => Action::Unsupported(command.to_owned()),
    }
}

fn usage_error(args: &[OsString], usage: &'static str) -> Action {
    let received = args
        .iter()
        .map(|argument| display_argument(argument))
        .collect::<Vec<_>>()
        .join(" ");
    let received = if received.is_empty() {
        "<none>".to_owned()
    } else {
        received
    };
    Action::Unsupported(format!(
        "invalid command usage\n  received: {received}\n  usage: {usage}\n  help: {USAGE_HELP}"
    ))
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
        "json2" => Some(LogFormat::Json2),
        "json2-signed" => Some(LogFormat::Json2Signed),
        _ => None,
    }
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
                format!(
                    "failed to read source `{label}`: {}",
                    diagnostic::stable_io_error(&error)
                ),
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
            return write_check_success(&label, options.format, true);
        }
        return write_diagnostics(&path, &source, &semantic_diagnostics, options);
    }
    write_diagnostics(&path, &source, &parsed.diagnostics, options)
}

fn inspect_path(path: PathBuf, format: OutputFormat) -> u8 {
    if path.is_dir() {
        return inspect_project_path(&path, format);
    }
    inspect_source_path(&path, format)
}

fn inspect_source_path(path: &std::path::Path, format: OutputFormat) -> u8 {
    let label = display_argument(path.as_os_str());
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-C003",
                format!(
                    "failed to read source `{label}`: {}",
                    diagnostic::stable_io_error(&error)
                ),
                diagnostic::Span::new(0, 0),
            );
            return write_inspection(path, "", "source", &[], &[diagnostic], format);
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
            return write_inspection(path, "", "source", &[], &[diagnostic], format);
        }
    };
    let parsed = parser::parse(&source);
    let root = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let files = parsed
        .program
        .as_ref()
        .map(|program| vec![analysis::file_context(path, root, program)])
        .unwrap_or_default();
    if !parsed.diagnostics.is_empty() {
        return write_inspection(path, &source, "source", &files, &parsed.diagnostics, format);
    }
    let Some(program) = parsed.program else {
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-C004",
            "parser returned no program without diagnostics",
            diagnostic::Span::new(0, 0),
        );
        return write_inspection(path, &source, "source", &files, &[diagnostic], format);
    };
    let diagnostics = semantic::check(&program);
    write_inspection(path, &source, "source", &files, &diagnostics, format)
}

fn inspect_project_path(path: &std::path::Path, format: OutputFormat) -> u8 {
    let loaded = match project::load(path) {
        Ok(loaded) => loaded,
        Err(diagnostic) => {
            return write_inspection(path, "", "project", &[], &[diagnostic], format);
        }
    };
    let diagnostics = semantic::check(&loaded.program);
    write_inspection(
        path,
        &loaded.source,
        "project",
        &loaded.analysis_files,
        &diagnostics,
        format,
    )
}

fn write_inspection(
    path: &std::path::Path,
    source: &str,
    kind: &str,
    files: &[analysis::FileContext],
    diagnostics: &[diagnostic::Diagnostic],
    format: OutputFormat,
) -> u8 {
    match format {
        OutputFormat::Human => {
            let summary = format!(
                "inspected {kind} {} files={} symbols={} imports={} diagnostics={}\n",
                display_argument(path.as_os_str()),
                files.len(),
                files.iter().map(|file| file.symbols.len()).sum::<usize>(),
                files.iter().map(|file| file.imports.len()).sum::<usize>(),
                diagnostics.len(),
            );
            let status = write_stdout(&summary);
            if status != EXIT_OK || diagnostics.is_empty() {
                return status;
            }
            write_diagnostics(
                path,
                source,
                diagnostics,
                Options {
                    format: OutputFormat::Human,
                    ..Options::default()
                },
            )
        }
        OutputFormat::Json => {
            let context = inspection_context_json(kind, files);
            let rendered = diagnostic::render_analysis_json(path, source, diagnostics, &context);
            let status = write_stdout(&rendered);
            if status == EXIT_OK && !diagnostics.is_empty() {
                EXIT_DATA
            } else {
                status
            }
        }
    }
}

fn inspection_context_json(kind: &str, files: &[analysis::FileContext]) -> json::JsonValue {
    let mut value = BTreeMap::from([
        (
            "files".to_owned(),
            json::JsonValue::Array(files.iter().map(inspection_file_json).collect()),
        ),
        ("kind".to_owned(), json::JsonValue::String(kind.to_owned())),
    ]);
    if !files.is_empty() {
        let pack = analysis::build_context(files, analysis::ContextLimits::default());
        value.insert("agent".to_owned(), inspection_context_pack_json(&pack));
    }
    json::JsonValue::Object(value)
}

fn inspection_context_pack_json(pack: &analysis::ContextPack) -> json::JsonValue {
    json::JsonValue::Object(BTreeMap::from([
        (
            "files".to_owned(),
            json::JsonValue::Array(
                pack.files
                    .iter()
                    .map(|file| {
                        json::JsonValue::Object(BTreeMap::from([
                            (
                                "path".to_owned(),
                                json::JsonValue::String(file.path.clone()),
                            ),
                            (
                                "symbols".to_owned(),
                                json::JsonValue::Array(
                                    file.symbols
                                        .iter()
                                        .map(|symbol| {
                                            json::JsonValue::Object(BTreeMap::from([
                                                (
                                                    "kind".to_owned(),
                                                    json::JsonValue::String(
                                                        symbol.kind.as_str().to_owned(),
                                                    ),
                                                ),
                                                (
                                                    "name".to_owned(),
                                                    json::JsonValue::String(symbol.name.clone()),
                                                ),
                                                (
                                                    "public".to_owned(),
                                                    json::JsonValue::Bool(symbol.public),
                                                ),
                                                (
                                                    "span".to_owned(),
                                                    json::JsonValue::Object(BTreeMap::from([
                                                        (
                                                            "end".to_owned(),
                                                            json::JsonValue::Number(
                                                                symbol.span.end.to_string(),
                                                            ),
                                                        ),
                                                        (
                                                            "start".to_owned(),
                                                            json::JsonValue::Number(
                                                                symbol.span.start.to_string(),
                                                            ),
                                                        ),
                                                    ])),
                                                ),
                                            ]))
                                        })
                                        .collect(),
                                ),
                            ),
                            (
                                "imports".to_owned(),
                                json::JsonValue::Array(
                                    file.imports
                                        .iter()
                                        .map(|import| {
                                            json::JsonValue::Object(BTreeMap::from([
                                                (
                                                    "alias".to_owned(),
                                                    import.alias.as_ref().map_or(
                                                        json::JsonValue::Null,
                                                        |alias| {
                                                            json::JsonValue::String(alias.clone())
                                                        },
                                                    ),
                                                ),
                                                (
                                                    "kind".to_owned(),
                                                    json::JsonValue::String(
                                                        import.kind.as_str().to_owned(),
                                                    ),
                                                ),
                                                (
                                                    "module".to_owned(),
                                                    json::JsonValue::String(import.module.clone()),
                                                ),
                                                (
                                                    "symbols".to_owned(),
                                                    json::JsonValue::Array(
                                                        import
                                                            .symbols
                                                            .iter()
                                                            .map(|symbol| {
                                                                json::JsonValue::String(
                                                                    symbol.clone(),
                                                                )
                                                            })
                                                            .collect(),
                                                    ),
                                                ),
                                            ]))
                                        })
                                        .collect(),
                                ),
                            ),
                            (
                                "total_imports".to_owned(),
                                json::JsonValue::Number(file.total_imports.to_string()),
                            ),
                            (
                                "total_symbols".to_owned(),
                                json::JsonValue::Number(file.total_symbols.to_string()),
                            ),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "omitted_files".to_owned(),
            json::JsonValue::Number(pack.omitted_files.to_string()),
        ),
        (
            "omitted_imports".to_owned(),
            json::JsonValue::Number(pack.omitted_imports.to_string()),
        ),
        (
            "omitted_symbols".to_owned(),
            json::JsonValue::Number(pack.omitted_symbols.to_string()),
        ),
        (
            "schema".to_owned(),
            json::JsonValue::String(pack.schema.to_owned()),
        ),
        (
            "total_files".to_owned(),
            json::JsonValue::Number(pack.total_files.to_string()),
        ),
        (
            "total_imports".to_owned(),
            json::JsonValue::Number(pack.total_imports.to_string()),
        ),
        (
            "total_symbols".to_owned(),
            json::JsonValue::Number(pack.total_symbols.to_string()),
        ),
        (
            "truncated".to_owned(),
            json::JsonValue::Bool(pack.truncated),
        ),
    ]))
}

fn inspection_file_json(file: &analysis::FileContext) -> json::JsonValue {
    json::JsonValue::Object(BTreeMap::from([
        (
            "imports".to_owned(),
            json::JsonValue::Array(
                file.imports
                    .iter()
                    .map(|import| {
                        json::JsonValue::Object(BTreeMap::from([
                            (
                                "alias".to_owned(),
                                import
                                    .alias
                                    .as_ref()
                                    .map_or(json::JsonValue::Null, |alias| {
                                        json::JsonValue::String(alias.clone())
                                    }),
                            ),
                            (
                                "kind".to_owned(),
                                json::JsonValue::String(import.kind.as_str().to_owned()),
                            ),
                            (
                                "module".to_owned(),
                                json::JsonValue::String(import.module.clone()),
                            ),
                            (
                                "symbols".to_owned(),
                                json::JsonValue::Array(
                                    import
                                        .symbols
                                        .iter()
                                        .map(|symbol| json::JsonValue::String(symbol.clone()))
                                        .collect(),
                                ),
                            ),
                        ]))
                    })
                    .collect(),
            ),
        ),
        (
            "path".to_owned(),
            json::JsonValue::String(file.path.clone()),
        ),
        (
            "symbols".to_owned(),
            json::JsonValue::Array(
                file.symbols
                    .iter()
                    .map(|symbol| {
                        json::JsonValue::Object(BTreeMap::from([
                            (
                                "end_byte".to_owned(),
                                json::JsonValue::Number(symbol.span.end.to_string()),
                            ),
                            (
                                "kind".to_owned(),
                                json::JsonValue::String(symbol.kind.as_str().to_owned()),
                            ),
                            (
                                "name".to_owned(),
                                json::JsonValue::String(symbol.name.clone()),
                            ),
                            ("public".to_owned(), json::JsonValue::Bool(symbol.public)),
                            (
                                "start_byte".to_owned(),
                                json::JsonValue::Number(symbol.span.start.to_string()),
                            ),
                        ]))
                    })
                    .collect(),
            ),
        ),
    ]))
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
    if let Err(error) = install_cancellation_handler() {
        return write_io_error("cancellation handler", error);
    }
    if let Some(status) = validate_log_options(options) {
        return status;
    }
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
    let mut event_sink = CliEventSink::new(
        options.log,
        Some(RunIdentity {
            program_sha256: sha256_hex(label.as_bytes()),
            manifest_sha256: None,
        }),
    );
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            if let Some(status) = finish_preflight_event(
                &mut event_sink,
                "SEL-C003",
                &format!(
                    "failed to read source `{label}`: {}",
                    diagnostic::stable_io_error(&error)
                ),
            ) {
                return status;
            }
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-C003",
                format!(
                    "failed to read source `{label}`: {}",
                    diagnostic::stable_io_error(&error)
                ),
                diagnostic::Span::new(0, 0),
            );
            return write_diagnostics_with_status(&path, "", &[diagnostic], options, EXIT_IO);
        }
    };
    event_sink.identity = Some(RunIdentity {
        program_sha256: sha256_hex(&bytes),
        manifest_sha256: None,
    });
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(error) => {
            let offset = error.utf8_error().valid_up_to();
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-L001",
                format!("source is not valid UTF-8 at byte offset {offset}"),
                diagnostic::Span::new(offset, offset.saturating_add(1)),
            );
            if let Some(status) =
                finish_preflight_event(&mut event_sink, diagnostic.code, &diagnostic.message)
            {
                return status;
            }
            return write_diagnostics(&path, "", &[diagnostic], options);
        }
    };

    let parsed = parser::parse(&source);
    if !parsed.diagnostics.is_empty() {
        if let Some(diagnostic) = parsed.diagnostics.first()
            && let Some(status) =
                finish_preflight_event(&mut event_sink, diagnostic.code, &diagnostic.message)
        {
            return status;
        }
        return write_diagnostics(&path, &source, &parsed.diagnostics, options);
    }
    let Some(program) = parsed.program else {
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-C004",
            "parser returned no program without diagnostics",
            diagnostic::Span::new(0, 0),
        );
        if let Some(status) =
            finish_preflight_event(&mut event_sink, diagnostic.code, &diagnostic.message)
        {
            return status;
        }
        return write_diagnostics(&path, &source, &[diagnostic], options);
    };
    let semantic_diagnostics = semantic::check(&program);
    if !semantic_diagnostics.is_empty() {
        if let Some(diagnostic) = semantic_diagnostics.first()
            && let Some(status) =
                finish_preflight_event(&mut event_sink, diagnostic.code, &diagnostic.message)
        {
            return status;
        }
        return write_diagnostics(&path, &source, &semantic_diagnostics, options);
    }

    let base_dir = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let execution =
        runtime::execute_with_context_and_events(&program, base_dir, 128, 4, &mut event_sink);
    match execution {
        Ok(output) => {
            event_sink.set_result_sha256(execution_output_sha256(&output));
            let result_failed = matches!(&output.value, runtime::Value::Result { ok: false, .. });
            let status = write_execution_output(
                &path,
                &source,
                &program,
                output,
                options,
                is_receipt_log(options.log) && event_sink.has_run(),
            );
            if status == EXIT_OK {
                event_sink.finish("ok", None, None);
            } else {
                let code = if result_failed && status == EXIT_DATA {
                    "SEL-R127"
                } else {
                    "SEL-C002"
                };
                event_sink.finish("error", Some(code), None);
            }
            if let Some(error) = event_sink.take_failure() {
                return write_io_error("event log", error);
            }
            status
        }
        Err(error) => {
            if event_sink.was_cancelled() {
                event_sink.finish(
                    "error",
                    Some(CANCELLATION_ERROR_CODE),
                    Some(CANCELLATION_MESSAGE),
                );
                if let Some(error) = event_sink.take_failure() {
                    return write_io_error("event log", error);
                }
                return write_cancellation_diagnostic(options);
            }
            event_sink.finish("error", Some(error.code), Some(&error.message));
            if let Some(error) = event_sink.take_failure() {
                return write_io_error("event log", error);
            }
            if is_receipt_log(options.log) && event_sink.has_run() {
                return EXIT_DATA;
            }
            let diagnostic = error.diagnostic();
            if options.format == OutputFormat::Human {
                let mut rendered = colorize_diagnostics(
                    diagnostic::render(&path, &source, &diagnostic),
                    options.color,
                );
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

fn finish_preflight_event(event_sink: &mut CliEventSink, code: &str, message: &str) -> Option<u8> {
    event_sink.record_preflight_error(code, message);
    event_sink.finish("error", Some(code), None);
    event_sink
        .take_failure()
        .map(|error| write_io_error("event log", error))
}

fn run_job(action: JobAction) -> u8 {
    if matches!(&action, JobAction::Run { .. })
        && let Err(error) = install_cancellation_handler()
    {
        return write_io_error("cancellation handler", error);
    }
    let (format, result) = match action {
        JobAction::Create {
            spec,
            id,
            root,
            format,
        } => (
            format,
            job::create(&PathBuf::from(root), &PathBuf::from(spec), id.as_deref()),
        ),
        JobAction::Status { id, root, format } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (format, job::status(&PathBuf::from(root), id))
        }
        JobAction::List { root, format } => (format, job::list(&PathBuf::from(root))),
        JobAction::Transition {
            id,
            status,
            root,
            format,
        } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            let Some(status) = status.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J006",
                        message: "job status must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (format, job::transition(&PathBuf::from(root), id, status))
        }
        JobAction::Retry { id, root, format } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (format, job::retry(&PathBuf::from(root), id))
        }
        JobAction::Claim {
            id,
            owner,
            lease_id,
            ttl_seconds,
            root,
            format,
        } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (
                format,
                job::claim(
                    &PathBuf::from(root),
                    id,
                    owner.as_deref(),
                    lease_id.as_deref(),
                    ttl_seconds,
                ),
            )
        }
        JobAction::Run {
            id,
            owner,
            binary,
            root,
            format,
        } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            let mut config = job::job_worker::WorkerConfig::default();
            if let Some(owner) = owner {
                let Some(owner) = owner.to_str() else {
                    return write_job_error(
                        job::JobError {
                            code: "SEL-J014",
                            message: "worker owner must be valid UTF-8".to_owned(),
                        },
                        format,
                    );
                };
                config.owner = owner.to_owned();
            }
            config.binary = binary.map(PathBuf::from);
            (
                format,
                job::job_worker::run(&PathBuf::from(root), id, config),
            )
        }
        JobAction::Heartbeat {
            id,
            lease_id,
            ttl_seconds,
            root,
            format,
        } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            let Some(lease_id) = lease_id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J017",
                        message: "lease id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (
                format,
                job::heartbeat(&PathBuf::from(root), id, lease_id, ttl_seconds),
            )
        }
        JobAction::Release {
            id,
            lease_id,
            root,
            format,
        } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            let Some(lease_id) = lease_id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J017",
                        message: "lease id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (format, job::release(&PathBuf::from(root), id, lease_id))
        }
        JobAction::Recover { id, root, format } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (format, job::recover_lost(&PathBuf::from(root), id))
        }
        JobAction::Preflight { id, root, format } => {
            let Some(id) = id.to_str() else {
                return write_job_error(
                    job::JobError {
                        code: "SEL-J003",
                        message: "job id must be valid UTF-8".to_owned(),
                    },
                    format,
                );
            };
            (format, job::preflight(&PathBuf::from(root), id))
        }
    };
    match result {
        Ok(value) => match format {
            OutputFormat::Human => write_stdout(&job::render_human(&value)),
            OutputFormat::Json => write_stdout(&format!("{}\n", json::encode(&value))),
        },
        Err(error) => write_job_error(error, format),
    }
}

fn check_json_data_path(path: &std::path::Path, options: Options) -> u8 {
    let label = display_argument(path.as_os_str());
    if let Err(error) = std::fs::read(path) {
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-C003",
            format!(
                "failed to read source `{label}`: {}",
                diagnostic::stable_io_error(&error)
            ),
            diagnostic::Span::new(0, 0),
        );
        return write_diagnostics(path, "", &[diagnostic], options);
    }
    if options.quiet {
        EXIT_OK
    } else {
        write_check_success(&label, options.format, false)
    }
}

fn run_json_data_path(path: &std::path::Path, options: Options) -> u8 {
    let label = display_argument(path.as_os_str());
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            let diagnostic = diagnostic::Diagnostic::new(
                "SEL-C003",
                format!(
                    "failed to read source `{label}`: {}",
                    diagnostic::stable_io_error(&error)
                ),
                diagnostic::Span::new(0, 0),
            );
            return write_diagnostics_with_status(path, "", &[diagnostic], options, EXIT_IO);
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
            write_check_success(&loaded.label, options.format, false)
        }
    } else {
        write_diagnostics(path, &loaded.source, &semantic_diagnostics, options)
    }
}

fn write_check_success(label: &str, format: OutputFormat, includes_semantics: bool) -> u8 {
    match format {
        OutputFormat::Human => {
            let suffix = if includes_semantics {
                " (syntax + semantics)"
            } else {
                ""
            };
            write_stdout(&format!("checked {label}{suffix}\n"))
        }
        OutputFormat::Json => {
            let mut value = BTreeMap::new();
            value.insert("checked".to_owned(), json::JsonValue::Bool(true));
            value.insert("path".to_owned(), json::JsonValue::String(label.to_owned()));
            value.insert(
                "schema".to_owned(),
                json::JsonValue::String("seloryn.check.v1".to_owned()),
            );
            value.insert(
                "status".to_owned(),
                json::JsonValue::String("ok".to_owned()),
            );
            write_stdout(&format!(
                "{}\n",
                json::encode(&json::JsonValue::Object(value))
            ))
        }
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
    if let Some(status) = validate_log_options(options) {
        return status;
    }
    let mut event_sink = CliEventSink::new(
        options.log,
        Some(RunIdentity {
            program_sha256: sha256_hex(path.to_string_lossy().as_bytes()),
            manifest_sha256: None,
        }),
    );
    let loaded = match project::load(path) {
        Ok(loaded) => loaded,
        Err(diagnostic) => {
            if let Some(status) =
                finish_preflight_event(&mut event_sink, diagnostic.code, &diagnostic.message)
            {
                return status;
            }
            return write_diagnostics(diagnostic_path, "", &[diagnostic], options);
        }
    };
    event_sink.identity = Some(RunIdentity {
        program_sha256: loaded.program_sha256.clone(),
        manifest_sha256: Some(loaded.manifest_sha256.clone()),
    });
    let semantic_diagnostics = semantic::check(&loaded.program);
    if !semantic_diagnostics.is_empty() {
        if let Some(diagnostic) = semantic_diagnostics.first()
            && let Some(status) =
                finish_preflight_event(&mut event_sink, diagnostic.code, &diagnostic.message)
        {
            return status;
        }
        return write_diagnostics(
            diagnostic_path,
            &loaded.source,
            &semantic_diagnostics,
            options,
        );
    }
    let execution = runtime::execute_with_capabilities_and_training_limits(
        &loaded.program,
        &loaded.base_dir,
        loaded.json_max_bytes,
        loaded.json_max_depth,
        loaded.training_json_max_bytes,
        loaded.training_json_max_depth,
        &loaded.read_roots,
        &loaded.write_roots,
        &mut event_sink,
    );
    match execution {
        Ok(output) => {
            event_sink.set_result_sha256(execution_output_sha256(&output));
            let result_failed = matches!(&output.value, runtime::Value::Result { ok: false, .. });
            let status = write_execution_output(
                diagnostic_path,
                &loaded.source,
                &loaded.program,
                output,
                options,
                is_receipt_log(options.log) && event_sink.has_run(),
            );
            if status == EXIT_OK {
                event_sink.finish("ok", None, None);
            } else {
                let code = if result_failed && status == EXIT_DATA {
                    "SEL-R127"
                } else {
                    "SEL-C002"
                };
                event_sink.finish("error", Some(code), None);
            }
            if let Some(error) = event_sink.take_failure() {
                return write_io_error("event log", error);
            }
            status
        }
        Err(error) => {
            if event_sink.was_cancelled() {
                event_sink.finish(
                    "error",
                    Some(CANCELLATION_ERROR_CODE),
                    Some(CANCELLATION_MESSAGE),
                );
                if let Some(error) = event_sink.take_failure() {
                    return write_io_error("event log", error);
                }
                return write_cancellation_diagnostic(options);
            }
            event_sink.finish("error", Some(error.code), Some(&error.message));
            if let Some(error) = event_sink.take_failure() {
                return write_io_error("event log", error);
            }
            if is_receipt_log(options.log) && event_sink.has_run() {
                return EXIT_DATA;
            }
            let diagnostic = error.diagnostic();
            if options.format == OutputFormat::Human {
                let mut rendered = colorize_diagnostics(
                    diagnostic::render(diagnostic_path, &loaded.source, &diagnostic),
                    options.color,
                );
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

fn validate_log_options(options: Options) -> Option<u8> {
    if options.log == LogFormat::Json2Signed
        && env::var_os("SELORYN_RECEIPT_KEY").is_none_or(|key| key.is_empty())
    {
        return Some(write_stderr(
            "SEL-C006 json2-signed requires a non-empty SELORYN_RECEIPT_KEY environment variable\n",
        ));
    }
    None
}

fn execution_output_sha256(output: &runtime::ExecutionOutput) -> String {
    let canonical = json::JsonValue::Object(BTreeMap::from([
        (
            "stdout".to_owned(),
            json::JsonValue::String(output.stdout.clone()),
        ),
        ("value".to_owned(), runtime::value_to_json(&output.value)),
    ]));
    sha256_hex(json::encode(&canonical).as_bytes())
}

fn write_execution_output(
    path: &std::path::Path,
    source: &str,
    program: &parser::Program,
    output: runtime::ExecutionOutput,
    options: Options,
    suppress_result_diagnostic: bool,
) -> u8 {
    let runtime::ExecutionOutput { value, stdout } = output;
    if let runtime::Value::Result { ok: false, .. } = &value {
        if !stdout.is_empty() {
            let status = write_stdout_for_run(&stdout, suppress_result_diagnostic);
            if status != EXIT_OK {
                return status;
            }
        }
        let diagnostic = diagnostic::Diagnostic::new(
            "SEL-R127",
            format!("program returned {}", value.render()),
            result_failure_span(program),
        );
        if suppress_result_diagnostic {
            return EXIT_DATA;
        }
        return write_diagnostics(path, source, &[diagnostic], options);
    }

    if options.format == OutputFormat::Json {
        let result = json::JsonValue::Object(BTreeMap::from([
            ("stdout".to_owned(), json::JsonValue::String(stdout)),
            ("value".to_owned(), runtime::value_to_json(&value)),
        ]));
        return write_stdout_for_run(
            &format!("{}\n", json::encode(&result)),
            suppress_result_diagnostic,
        );
    }

    let mut rendered = stdout;
    if value != runtime::Value::Unit {
        rendered.push_str(&format!("{}\n", render_cli_value(program, value)));
    }
    if rendered.is_empty() {
        EXIT_OK
    } else {
        write_stdout_for_run(&rendered, suppress_result_diagnostic)
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
    let bundle_sha256 = sha256_hex(&bytes);
    let build_metadata = json::JsonValue::Object(BTreeMap::from([
        (
            "schema".to_owned(),
            json::JsonValue::String("seloryn.build.v1".to_owned()),
        ),
        (
            "bundle_format".to_owned(),
            json::JsonValue::String(project::BUNDLE_FORMAT.to_owned()),
        ),
        (
            "language_version".to_owned(),
            json::JsonValue::String(LANGUAGE_VERSION.to_owned()),
        ),
        (
            "runtime_package".to_owned(),
            json::JsonValue::String(PACKAGE_VERSION.to_owned()),
        ),
        (
            "profile".to_owned(),
            json::JsonValue::String(profile.clone()),
        ),
        (
            "artifact".to_owned(),
            json::JsonValue::String("seloryn.selyapp".to_owned()),
        ),
        (
            "bytes".to_owned(),
            json::JsonValue::Number(bytes.len().to_string()),
        ),
        (
            "sha256".to_owned(),
            json::JsonValue::String(bundle_sha256.clone()),
        ),
        (
            "integrity".to_owned(),
            json::JsonValue::String("sha256".to_owned()),
        ),
        (
            "authenticity".to_owned(),
            json::JsonValue::String("none".to_owned()),
        ),
        ("deterministic".to_owned(), json::JsonValue::Bool(true)),
    ]));
    let metadata_path = output.join("seloryn-build.json");
    if let Err(error) = fs::write(
        &metadata_path,
        format!("{}\n", json::encode(&build_metadata)),
    ) {
        return write_io_diagnostic(&format!(
            "SEL-C003 failed to write build metadata `{}`: {error}\n",
            display_argument(metadata_path.as_os_str())
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
    result.insert(
        "bundle_format".to_owned(),
        json::JsonValue::String(project::BUNDLE_FORMAT.to_owned()),
    );
    result.insert("profile".to_owned(), json::JsonValue::String(profile));
    result.insert("sha256".to_owned(), json::JsonValue::String(bundle_sha256));
    result.insert(
        "metadata".to_owned(),
        json::JsonValue::String("seloryn-build.json".to_owned()),
    );
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
    if path.is_dir()
        && let Err(diagnostic) = verify_build_metadata(&path, &bytes)
    {
        return write_diagnostics(&artifact, "", &[diagnostic], Options::default());
    }
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

fn verify_build_metadata(
    directory: &std::path::Path,
    bundle_bytes: &[u8],
) -> Result<(), diagnostic::Diagnostic> {
    let metadata_path = directory.join("seloryn-build.json");
    if !metadata_path.is_file() {
        return Ok(());
    }
    let source = fs::read_to_string(&metadata_path).map_err(|error| {
        diagnostic::Diagnostic::new(
            "SEL-B001",
            format!("bundle build metadata is unreadable: {error}"),
            diagnostic::Span::new(0, 0),
        )
    })?;
    let value = json::parse(&source, 64 * 1024, 8).map_err(|error| {
        diagnostic::Diagnostic::new(
            "SEL-B001",
            format!("bundle build metadata is invalid: {}", error.message),
            diagnostic::Span::new(0, 0),
        )
    })?;
    let json::JsonValue::Object(object) = value else {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata must be a JSON object",
            diagnostic::Span::new(0, 0),
        ));
    };
    const ALLOWED_FIELDS: &[&str] = &[
        "schema",
        "bundle_format",
        "language_version",
        "runtime_package",
        "profile",
        "artifact",
        "bytes",
        "sha256",
        "integrity",
        "authenticity",
        "deterministic",
    ];
    if let Some(field) = object
        .keys()
        .find(|field| !ALLOWED_FIELDS.contains(&field.as_str()))
    {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            format!("bundle build metadata contains unsupported field `{field}`"),
            diagnostic::Span::new(0, 0),
        ));
    }
    let string_field = |key: &str| -> Result<&str, diagnostic::Diagnostic> {
        match object.get(key) {
            Some(json::JsonValue::String(value)) => Ok(value),
            Some(_) => Err(diagnostic::Diagnostic::new(
                "SEL-B001",
                format!("bundle build metadata field `{key}` must be a string"),
                diagnostic::Span::new(0, 0),
            )),
            None => Err(diagnostic::Diagnostic::new(
                "SEL-B001",
                format!("bundle build metadata is missing `{key}`"),
                diagnostic::Span::new(0, 0),
            )),
        }
    };
    if string_field("schema")? != "seloryn.build.v1" {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata schema is unsupported",
            diagnostic::Span::new(0, 0),
        ));
    }
    if string_field("bundle_format")? != project::BUNDLE_FORMAT {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata format is unsupported",
            diagnostic::Span::new(0, 0),
        ));
    }
    if string_field("language_version")? != LANGUAGE_VERSION {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata language version is incompatible",
            diagnostic::Span::new(0, 0),
        ));
    }
    if string_field("runtime_package")? != PACKAGE_VERSION {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata runtime version is incompatible",
            diagnostic::Span::new(0, 0),
        ));
    }
    if !matches!(string_field("profile")?, "debug" | "release") {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata profile is unsupported",
            diagnostic::Span::new(0, 0),
        ));
    }
    if string_field("artifact")? != "seloryn.selyapp" {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata artifact is unsupported",
            diagnostic::Span::new(0, 0),
        ));
    }
    if string_field("integrity")? != "sha256" {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata integrity algorithm is unsupported",
            diagnostic::Span::new(0, 0),
        ));
    }
    if string_field("authenticity")? != "none" {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata authenticity is unsupported; SHA-256 does not authenticate authorship",
            diagnostic::Span::new(0, 0),
        ));
    }
    if !matches!(
        object.get("deterministic"),
        Some(json::JsonValue::Bool(true))
    ) {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata deterministic flag is invalid",
            diagnostic::Span::new(0, 0),
        ));
    }
    let expected_bytes = bundle_bytes.len().to_string();
    match object.get("bytes") {
        Some(json::JsonValue::Number(value)) if value == &expected_bytes => {}
        Some(_) => {
            return Err(diagnostic::Diagnostic::new(
                "SEL-B001",
                "bundle build metadata byte length does not match the artifact",
                diagnostic::Span::new(0, 0),
            ));
        }
        None => {
            return Err(diagnostic::Diagnostic::new(
                "SEL-B001",
                "bundle build metadata is missing `bytes`",
                diagnostic::Span::new(0, 0),
            ));
        }
    }
    if string_field("sha256")? != sha256_hex(bundle_bytes) {
        return Err(diagnostic::Diagnostic::new(
            "SEL-B001",
            "bundle build metadata SHA-256 does not match the artifact",
            diagnostic::Span::new(0, 0),
        ));
    }
    Ok(())
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

fn write_stdout_for_run(message: &str, suppress_diagnostic: bool) -> u8 {
    match io::stdout().write_all(message.as_bytes()) {
        Ok(()) => EXIT_OK,
        Err(_error) if suppress_diagnostic => EXIT_IO,
        Err(error) => write_io_error("stdout", error),
    }
}

fn write_cancellation_diagnostic(options: Options) -> u8 {
    if is_receipt_log(options.log) {
        return EXIT_CANCELLED;
    }
    let message = if options.format == OutputFormat::Json {
        json::encode(&json::JsonValue::Object(BTreeMap::from([
            (
                "code".to_owned(),
                json::JsonValue::String(CANCELLATION_ERROR_CODE.to_owned()),
            ),
            (
                "message".to_owned(),
                json::JsonValue::String(CANCELLATION_MESSAGE.to_owned()),
            ),
        ]))) + "\n"
    } else {
        format!("{CANCELLATION_ERROR_CODE} {CANCELLATION_MESSAGE}\n")
    };
    match io::stderr().write_all(message.as_bytes()) {
        Ok(()) => EXIT_CANCELLED,
        Err(error) => write_io_error("stderr", error),
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

fn write_job_error(error: job::JobError, format: OutputFormat) -> u8 {
    let message = match format {
        OutputFormat::Human => format!("{} {}\n", error.code, error.message),
        OutputFormat::Json => {
            let value = json::JsonValue::Object(BTreeMap::from([
                (
                    "code".to_owned(),
                    json::JsonValue::String(error.code.to_owned()),
                ),
                ("message".to_owned(), json::JsonValue::String(error.message)),
                (
                    "schema".to_owned(),
                    json::JsonValue::String("seloryn.error.v1".to_owned()),
                ),
            ]));
            format!("{}\n", json::encode(&value))
        }
    };
    if error.code == "SEL-J001" || error.code == "SEL-J010" {
        match io::stderr().write_all(message.as_bytes()) {
            Ok(()) => EXIT_IO,
            Err(write_error) => write_io_error("stderr", write_error),
        }
    } else {
        write_data_stderr(&message)
    }
}

fn write_diagnostics(
    path: &std::path::Path,
    source: &str,
    diagnostics: &[diagnostic::Diagnostic],
    options: Options,
) -> u8 {
    write_diagnostics_with_status(path, source, diagnostics, options, EXIT_DATA)
}

fn write_diagnostics_with_status(
    path: &std::path::Path,
    source: &str,
    diagnostics: &[diagnostic::Diagnostic],
    options: Options,
    status: u8,
) -> u8 {
    let rendered = match options.format {
        OutputFormat::Human => colorize_diagnostics(
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic::render(path, source, diagnostic))
                .collect::<String>(),
            options.color,
        ),
        OutputFormat::Json => diagnostic::render_json(path, source, diagnostics),
    };
    match io::stderr().write_all(rendered.as_bytes()) {
        Ok(()) => status,
        Err(error) => write_io_error("stderr", error),
    }
}

fn colorize_diagnostics(rendered: String, mode: ColorMode) -> String {
    let enabled = match mode {
        ColorMode::Always => true,
        ColorMode::Never => false,
        ColorMode::Auto => io::stderr().is_terminal(),
    };
    if !enabled {
        return rendered;
    }
    rendered
        .lines()
        .map(|line| {
            let Some(start) = line.find("SEL-") else {
                return line.to_owned();
            };
            let end = line[start..]
                .find(|character: char| character.is_whitespace())
                .map_or(line.len(), |offset| start + offset);
            format!(
                "{}\x1b[1;31m{}\x1b[0m{}",
                &line[..start],
                &line[start..end],
                &line[end..]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + if rendered.ends_with('\n') { "\n" } else { "" }
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
    use super::output::redact_sensitive_json;
    use super::{Action, ColorMode, classify, colorize_diagnostics, display_argument};
    use crate::json::JsonValue;
    use std::collections::BTreeMap;
    use std::ffi::OsString;

    #[test]
    fn no_arguments_open_help() {
        assert!(matches!(classify(&[]), Action::Help));
    }

    #[test]
    fn audit_payload_redacts_sensitive_fields_recursively() {
        let mut payload = JsonValue::Object(BTreeMap::from([
            (
                "token".to_owned(),
                JsonValue::String("do-not-persist".to_owned()),
            ),
            (
                "nested".to_owned(),
                JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
                    (
                        "api_key".to_owned(),
                        JsonValue::String("also-secret".to_owned()),
                    ),
                    ("safe".to_owned(), JsonValue::String("visible".to_owned())),
                ]))]),
            ),
        ]));
        redact_sensitive_json(&mut payload);
        let JsonValue::Object(object) = payload else {
            panic!("expected object")
        };
        assert_eq!(object["token"], JsonValue::String("<redacted>".to_owned()));
        assert_eq!(
            object["nested"],
            JsonValue::Array(vec![JsonValue::Object(BTreeMap::from([
                (
                    "api_key".to_owned(),
                    JsonValue::String("<redacted>".to_owned()),
                ),
                ("safe".to_owned(), JsonValue::String("visible".to_owned()),),
            ]))])
        );
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
            classify(&[
                OsString::from("inspect"),
                OsString::from("project"),
                OsString::from("--format=json")
            ]),
            Action::Inspect { .. }
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
        assert!(matches!(
            classify(&[OsString::from("doctor"), OsString::from("project")]),
            Action::Doctor { .. }
        ));
        assert!(matches!(
            classify(&[
                OsString::from("init"),
                OsString::from("project"),
                OsString::from("--template=tabular-mlp"),
                OsString::from("--format=json"),
            ]),
            Action::Init { .. }
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
            Action::Unsupported(message) => {
                assert!(message.contains("received: 入口"), "{message}");
                assert!(message.contains("usage: seloryn --help"), "{message}");
            }
            Action::Help | Action::Version | Action::Check { .. } | Action::Inspect { .. } => {
                panic!("expected unsupported action")
            }
            Action::Devices { .. }
            | Action::GpuProbe { .. }
            | Action::Doctor { .. }
            | Action::Init { .. }
            | Action::Run { .. }
            | Action::Build { .. }
            | Action::Exec { .. }
            | Action::JobHelp
            | Action::Job(_)
            | Action::Lsp => {
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

    #[test]
    fn color_mode_is_explicit_and_never_colors_json_or_pipes() {
        let human = "file.sely:1:1: SEL-C003 path not found\n".to_owned();
        assert!(colorize_diagnostics(human.clone(), ColorMode::Always).contains("\u{1b}[1;31m"));
        assert_eq!(
            colorize_diagnostics(human, ColorMode::Never),
            "file.sely:1:1: SEL-C003 path not found\n"
        );
    }
}
