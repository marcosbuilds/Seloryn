#[allow(dead_code)]
mod distribution;
#[allow(dead_code)]
mod json;

use distribution::{DistributionError, InstalledVersion};
use std::env;
use std::ffi::OsString;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<u8, DistributionError> {
    let root = distribution::root_dir()?;
    let mut arguments = env::args_os();
    let program = arguments
        .next()
        .unwrap_or_else(|| OsString::from("seloryn"));
    let remaining: Vec<OsString> = arguments.collect();
    match remaining.as_slice() {
        [command, version] if command == "use" => {
            distribution::set_default(
                &root,
                version.to_str().ok_or_else(|| DistributionError {
                    code: "SEL-D016",
                    message: "version argument is not valid UTF-8".to_owned(),
                })?,
            )?;
            println!("seloryn default version={}", version.to_string_lossy());
            Ok(0)
        }
        [command, version] if command == "uninstall" => {
            distribution::uninstall(
                &root,
                version.to_str().ok_or_else(|| DistributionError {
                    code: "SEL-D016",
                    message: "version argument is not valid UTF-8".to_owned(),
                })?,
            )?;
            println!("seloryn uninstalled version={}", version.to_string_lossy());
            Ok(0)
        }
        [command] if command == "versions" => print_versions(&root),
        _ => {
            let requested = distribution::invoked_version(&program)?;
            let installed = distribution::resolve_version(&root, requested.as_deref())?;
            dispatch(&installed, &remaining)
        }
    }
}

fn print_versions(root: &std::path::Path) -> Result<u8, DistributionError> {
    let state = distribution::read_state(root)?;
    let versions = distribution::list(root)?;
    if versions.is_empty() {
        println!("seloryn no installed versions");
        return Ok(0);
    }
    for version in versions {
        let marker = if state.default_version.as_deref() == Some(version.version.as_str()) {
            "*"
        } else {
            " "
        };
        println!(
            "{marker} {} language={} sha256={}",
            version.version, version.language_version, version.sha256
        );
    }
    Ok(0)
}

fn dispatch(installed: &InstalledVersion, arguments: &[OsString]) -> Result<u8, DistributionError> {
    let status = Command::new(&installed.runtime_path)
        .args(arguments)
        .status()
        .map_err(|error| {
            DistributionError::new(
                "SEL-D017",
                format!(
                    "failed to start {}: {error}",
                    installed.runtime_path.display()
                ),
            )
        })?;
    Ok(status.code().unwrap_or(1).clamp(0, 255) as u8)
}
