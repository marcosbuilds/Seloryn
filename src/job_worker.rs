//! Bounded local execution for an already validated `seloryn-run.v1` job.
//!
//! The worker deliberately accepts no shell, command line string, environment
//! map, working-directory override, or arbitrary executable. It invokes the
//! Seloryn binary by absolute path with the validated source as a single path
//! argument. The lease is the fencing token: every executor-owned transition
//! and heartbeat is authenticated by that token, while administrative
//! transitions cannot manufacture executor states. Every child is also
//! attached to an OS-level process-group/job-object fence so timeout and
//! cancellation do not intentionally leave a child tree behind.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use super::{
    JobError, JsonValue, Status, WorkerInput, atomic_write, ensure_job_contained,
    executor_transition, heartbeat, job_directory, preflight, worker_input,
};

const POLL_STEP: Duration = Duration::from_millis(10);
const MIN_LEASE_SECONDS: u64 = 5;
const LEASE_GRACE_SECONDS: u64 = 2;
const MAX_CAPTURE_BYTES: usize = super::MAX_RESULT_BYTES;
const RESULT_FILE: &str = "result.json";
const RESULT_SCHEMA: &str = "seloryn.job-result.v1";

#[derive(Clone, Debug)]
pub(crate) struct WorkerConfig {
    pub(crate) owner: String,
    pub(crate) binary: Option<PathBuf>,
    pub(crate) cancel: Arc<AtomicBool>,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            owner: format!("worker-{}", std::process::id()),
            binary: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[derive(Clone, Debug)]
struct RunOutcome {
    status: WorkerStatus,
    exit_code: Option<i32>,
    timed_out: bool,
    cancelled: bool,
    duration_ms: u128,
    fence_kind: String,
    stdout: Capture,
    stderr: Capture,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WorkerStatus {
    Succeeded,
    Failed,
    Cancelled,
}

impl WorkerStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Clone, Debug)]
struct Capture {
    bytes: usize,
    sha256: String,
    truncated: bool,
}

struct ManagedChild {
    child: Child,
    fence: ProcessFence,
}

impl ManagedChild {
    fn terminate(&mut self) -> Result<(), JobError> {
        self.fence.terminate()
    }

    fn wait(&mut self) -> Result<ExitStatus, JobError> {
        self.child
            .wait()
            .map_err(|error| JobError::io("SEL-J025", "failed to reap Seloryn job", error))
    }
}

#[cfg(windows)]
struct ProcessFence {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(unix)]
struct ProcessFence {
    pgid: i32,
}

#[cfg(not(any(unix, windows)))]
struct ProcessFence;

impl ProcessFence {
    fn attach(child: &Child) -> Result<Self, JobError> {
        #[cfg(windows)]
        {
            use std::mem::size_of;
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            };

            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return Err(JobError::io(
                    "SEL-J026",
                    "failed to create Windows process fence",
                    io::Error::last_os_error(),
                ));
            }
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let configured = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if configured == 0 {
                unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
                return Err(JobError::io(
                    "SEL-J026",
                    "failed to configure Windows process fence",
                    io::Error::last_os_error(),
                ));
            }
            let assigned = unsafe { AssignProcessToJobObject(handle, child.as_raw_handle()) };
            if assigned == 0 {
                unsafe { windows_sys::Win32::Foundation::CloseHandle(handle) };
                return Err(JobError::io(
                    "SEL-J026",
                    "failed to attach child to Windows process fence",
                    io::Error::last_os_error(),
                ));
            }
            Ok(Self { handle })
        }

        #[cfg(unix)]
        {
            Ok(Self {
                pgid: child.id() as i32,
            })
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = child;
            Err(JobError::new(
                "SEL-J026",
                "Seloryn job process fencing is unsupported on this platform",
            ))
        }
    }

    fn terminate(&self) -> Result<(), JobError> {
        #[cfg(windows)]
        {
            let result = unsafe {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle, 1)
            };
            if result == 0 {
                return Err(JobError::io(
                    "SEL-J025",
                    "failed to terminate Windows process fence",
                    io::Error::last_os_error(),
                ));
            }
            Ok(())
        }

        #[cfg(unix)]
        {
            const SIGKILL: i32 = 9;
            let result = unsafe { kill(-self.pgid, SIGKILL) };
            if result != 0 {
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(JobError::io(
                        "SEL-J025",
                        "failed to terminate Seloryn process group",
                        error,
                    ));
                }
            }
            Ok(())
        }

        #[cfg(not(any(unix, windows)))]
        {
            Err(JobError::new(
                "SEL-J025",
                "Seloryn job process fencing is unsupported on this platform",
            ))
        }
    }

    fn kind(&self) -> &'static str {
        #[cfg(windows)]
        {
            "windows-job-object"
        }
        #[cfg(unix)]
        {
            "posix-process-group"
        }
        #[cfg(not(any(unix, windows)))]
        {
            "unsupported"
        }
    }
}

impl Drop for ProcessFence {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = self.terminate();
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }

        #[cfg(unix)]
        {
            let _ = self.terminate();
        }
    }
}

#[cfg(unix)]
unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
}

/// Execute one local job to completion.
///
/// This is intentionally a coordinator-facing API rather than a CLI action.
/// The coordinator must provide a root and job id; the worker performs a
/// read-only preflight before claiming, revalidates the source after claiming,
/// and only then starts the trusted Seloryn executable.
pub(crate) fn run(root: &Path, job_id: &str, config: WorkerConfig) -> Result<JsonValue, JobError> {
    super::validate_job_id(job_id)?;
    super::validate_identity(&config.owner, "worker owner")?;
    let binary = resolve_binary(config.binary.as_deref())?;

    let ready = match preflight(root, job_id) {
        Ok(ready) => ready,
        Err(error) => {
            // A preflight rejection happens before a lease is acquired. Keep
            // the original CLI error, but persist a bounded receipt when the
            // job is still runnable so operators can inspect the failure
            // without reconstructing stderr or exposing its message.
            let _ = write_preflight_failure_receipt(root, job_id, &error);
            return Err(error);
        }
    };
    let timeout_ms = field_u64(&ready, "timeout_ms")?;
    let lease_seconds = lease_seconds(timeout_ms)?;
    let owner = std::ffi::OsString::from(config.owner.clone());
    let claimed = super::claim(root, job_id, Some(owner.as_os_str()), None, lease_seconds)?;
    let lease_id = field_string(&claimed, "lease_id")?.to_owned();
    let input = match worker_input(root, job_id, &lease_id) {
        Ok(input) => input,
        Err(error) => {
            if let Ok(spec_sha256) = field_string(&claimed, "spec_sha256") {
                let _ = write_failure_receipt(root, job_id, spec_sha256, &error, false);
                let _ = write_failure_receipt(root, job_id, spec_sha256, &error, true);
                let _ = fail_starting(root, job_id, &lease_id, &error.message);
            }
            return Err(error);
        }
    };
    let result = match spawn_seloryn(&binary, root, &input) {
        Ok(child) => execute_child(root, job_id, &lease_id, &input, child, &config),
        Err(error) => {
            let outcome = RunOutcome {
                status: WorkerStatus::Failed,
                exit_code: None,
                timed_out: false,
                cancelled: false,
                duration_ms: 0,
                fence_kind: "not-started".to_owned(),
                stdout: Capture::empty(),
                stderr: Capture::from_error(&error),
            };
            finish_failure(root, job_id, &lease_id, &input, &outcome, &error.message)?;
            Err(error)
        }
    }?;
    Ok(result)
}

fn resolve_binary(requested: Option<&Path>) -> Result<PathBuf, JobError> {
    let path = match requested {
        Some(path) => path.to_path_buf(),
        None => std::env::current_exe().map_err(|error| {
            JobError::io("SEL-J023", "failed to locate Seloryn executable", error)
        })?,
    };
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| JobError::io("SEL-J023", "failed to inspect Seloryn executable", error))?;
    if super::metadata_is_reparse(&metadata) || !metadata.is_file() {
        return Err(JobError::new(
            "SEL-J023",
            "Seloryn executable must be a regular non-reparse file",
        ));
    }
    let canonical = fs::canonicalize(&path)
        .map_err(|error| JobError::io("SEL-J023", "failed to resolve Seloryn executable", error))?;
    let name = canonical
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.eq_ignore_ascii_case("seloryn") {
        return Err(JobError::new(
            "SEL-J023",
            "worker executable must be named seloryn",
        ));
    }
    Ok(canonical)
}

fn lease_seconds(timeout_ms: u64) -> Result<u64, JobError> {
    if timeout_ms == 0 {
        return Err(JobError::new("SEL-J022", "worker timeout must be positive"));
    }
    let requested = timeout_ms
        .saturating_add(999)
        .checked_div(1000)
        .unwrap_or(u64::MAX)
        .saturating_add(LEASE_GRACE_SECONDS)
        .max(MIN_LEASE_SECONDS);
    // The heartbeat keeps a one-hour job alive; grace is best effort and may
    // be clipped at the v1 lease ceiling instead of rejecting the valid
    // maximum `timeout_ms` accepted by the job spec parser.
    Ok(requested.min(3600))
}

fn spawn_seloryn(
    binary: &Path,
    root: &Path,
    input: &WorkerInput,
) -> Result<ManagedChild, JobError> {
    let mut command = Command::new(binary);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let child = command
        .arg("run")
        .arg(&input.source)
        .arg("--format=json")
        .current_dir(root)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| JobError::io("SEL-J024", "failed to start Seloryn job", error))?;
    let fence = match ProcessFence::attach(&child) {
        Ok(fence) => fence,
        Err(error) => {
            let mut child = child;
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    Ok(ManagedChild { child, fence })
}

fn execute_child(
    root: &Path,
    job_id: &str,
    lease_id: &str,
    input: &WorkerInput,
    mut child: ManagedChild,
    config: &WorkerConfig,
) -> Result<JsonValue, JobError> {
    if let Err(error) = executor_transition(root, job_id, lease_id, Status::Running) {
        let _ = child.terminate();
        let _ = child.wait();
        return Err(error);
    }
    let stdout = child
        .child
        .stdout
        .take()
        .ok_or_else(|| JobError::new("SEL-J024", "Seloryn stdout pipe was not created"));
    let stdout = match stdout {
        Ok(stdout) => stdout,
        Err(error) => {
            let _ = child.terminate();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stderr = child
        .child
        .stderr
        .take()
        .ok_or_else(|| JobError::new("SEL-J024", "Seloryn stderr pipe was not created"));
    let stderr = match stderr {
        Ok(stderr) => stderr,
        Err(error) => {
            let _ = child.terminate();
            let _ = child.wait();
            return Err(error);
        }
    };
    let stdout_thread = thread::spawn(|| capture(stdout));
    let stderr_thread = thread::spawn(|| capture(stderr));
    let started = Instant::now();
    let heartbeat_every = Duration::from_secs(1).min(input.timeout / 3).max(POLL_STEP);
    let mut next_heartbeat = Instant::now() + heartbeat_every;
    let mut termination = None;
    let mut timed_out = false;
    let mut cancelled = false;
    let status_result: Result<ExitStatus, JobError> =
        (|| {
            loop {
                if let Some(status) = child.child.try_wait().map_err(|error| {
                    JobError::io("SEL-J024", "failed to poll Seloryn job", error)
                })? {
                    break Ok(status);
                }
                if config.cancel.load(Ordering::Acquire) || crate::cancellation_requested() {
                    termination = Some(WorkerStatus::Cancelled);
                    cancelled = true;
                    break terminate_child(root, job_id, lease_id, &mut child);
                }
                if started.elapsed() >= input.timeout {
                    termination = Some(WorkerStatus::Cancelled);
                    timed_out = true;
                    break terminate_child(root, job_id, lease_id, &mut child);
                }
                if Instant::now() >= next_heartbeat {
                    heartbeat(root, job_id, lease_id, lease_seconds_for(input.timeout))?;
                    next_heartbeat = Instant::now() + heartbeat_every;
                }
                thread::sleep(POLL_STEP);
            }
        })();
    let status = match status_result {
        Ok(status) => status,
        Err(error) => {
            // Polling, heartbeat and cancellation failures must not abandon
            // the direct child. Reap it before returning the lease error.
            let _ = child.terminate();
            let _ = child.wait();
            let _ = stdout_thread.join();
            let _ = stderr_thread.join();
            return Err(error);
        }
    };
    let stdout = join_capture(stdout_thread)?;
    let stderr = join_capture(stderr_thread)?;
    let outcome = RunOutcome {
        status: termination.unwrap_or_else(|| {
            if status.success() {
                WorkerStatus::Succeeded
            } else {
                WorkerStatus::Failed
            }
        }),
        exit_code: status.code(),
        timed_out,
        cancelled,
        duration_ms: started.elapsed().as_millis(),
        fence_kind: child.fence.kind().to_owned(),
        stdout,
        stderr,
    };
    if outcome.status == WorkerStatus::Cancelled {
        let result = write_result(root, job_id, input, &outcome, true)?;
        executor_transition(root, job_id, lease_id, Status::Cancelled)?;
        Ok(result)
    } else {
        executor_transition(root, job_id, lease_id, Status::Committing)?;
        let _pending = write_result(root, job_id, input, &outcome, false)?;
        let result = write_result(root, job_id, input, &outcome, true)?;
        let final_status = if outcome.status == WorkerStatus::Succeeded {
            Status::Succeeded
        } else {
            Status::Failed
        };
        executor_transition(root, job_id, lease_id, final_status)?;
        Ok(result)
    }
}

fn lease_seconds_for(timeout: Duration) -> u64 {
    lease_seconds(timeout.as_millis().min(u64::MAX as u128) as u64).unwrap_or(3600)
}

fn terminate_child(
    root: &Path,
    job_id: &str,
    lease_id: &str,
    child: &mut ManagedChild,
) -> Result<ExitStatus, JobError> {
    executor_transition(root, job_id, lease_id, Status::Cancelling)?;
    child.terminate()?;
    let status = child.wait()?;
    Ok(status)
}

fn capture<R: Read>(mut reader: R) -> Result<Capture, io::Error> {
    let mut captured = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut total = 0usize;
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read);
        if captured.len() < MAX_CAPTURE_BYTES {
            let keep = (MAX_CAPTURE_BYTES - captured.len()).min(read);
            captured.extend_from_slice(&buffer[..keep]);
            truncated |= keep < read;
        } else {
            truncated = true;
        }
    }
    Ok(Capture {
        bytes: total,
        sha256: crate::sha256_hex(&captured),
        truncated,
    })
}

fn join_capture(
    handle: thread::JoinHandle<Result<Capture, io::Error>>,
) -> Result<Capture, JobError> {
    handle
        .join()
        .map_err(|_| JobError::new("SEL-J024", "output capture thread panicked"))?
        .map_err(|error| JobError::io("SEL-J024", "failed to capture Seloryn output", error))
}

fn write_result(
    root: &Path,
    job_id: &str,
    input: &WorkerInput,
    outcome: &RunOutcome,
    committed: bool,
) -> Result<JsonValue, JobError> {
    let directory = job_directory(root, job_id);
    ensure_job_contained(root, &directory)?;
    let result = JsonValue::Object(BTreeMap::from([
        (
            "attempt".to_owned(),
            JsonValue::Number(input.attempt.to_string()),
        ),
        ("cancelled".to_owned(), JsonValue::Bool(outcome.cancelled)),
        ("committed".to_owned(), JsonValue::Bool(committed)),
        (
            "duration_ms".to_owned(),
            JsonValue::Number(outcome.duration_ms.to_string()),
        ),
        (
            "exit_code".to_owned(),
            outcome
                .exit_code
                .map(|code| JsonValue::Number(code.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "fence_kind".to_owned(),
            JsonValue::String(outcome.fence_kind.clone()),
        ),
        ("job_id".to_owned(), JsonValue::String(job_id.to_owned())),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(input.spec_sha256.clone()),
        ),
        (
            "stderr_bytes".to_owned(),
            JsonValue::Number(outcome.stderr.bytes.to_string()),
        ),
        (
            "stderr_sha256".to_owned(),
            JsonValue::String(outcome.stderr.sha256.clone()),
        ),
        (
            "stderr_truncated".to_owned(),
            JsonValue::Bool(outcome.stderr.truncated),
        ),
        (
            "stdout_bytes".to_owned(),
            JsonValue::Number(outcome.stdout.bytes.to_string()),
        ),
        (
            "stdout_sha256".to_owned(),
            JsonValue::String(outcome.stdout.sha256.clone()),
        ),
        (
            "stdout_truncated".to_owned(),
            JsonValue::Bool(outcome.stdout.truncated),
        ),
        (
            "status".to_owned(),
            JsonValue::String(outcome.status.as_str().to_owned()),
        ),
        ("timed_out".to_owned(), JsonValue::Bool(outcome.timed_out)),
        (
            "schema".to_owned(),
            JsonValue::String(RESULT_SCHEMA.to_owned()),
        ),
    ]));
    let encoded = format!("{}\n", super::json::encode(&result));
    atomic_write(&directory.join(RESULT_FILE), encoded.as_bytes())?;
    Ok(result)
}

fn finish_failure(
    root: &Path,
    job_id: &str,
    lease_id: &str,
    input: &WorkerInput,
    outcome: &RunOutcome,
    _message: &str,
) -> Result<(), JobError> {
    let _ = write_result(root, job_id, input, outcome, false)?;
    let _ = write_result(root, job_id, input, outcome, true)?;
    executor_transition(root, job_id, lease_id, Status::Failed)?;
    Ok(())
}

fn write_failure_receipt(
    root: &Path,
    job_id: &str,
    spec_sha256: &str,
    error: &JobError,
    committed: bool,
) -> Result<(), JobError> {
    let directory = job_directory(root, job_id);
    ensure_job_contained(root, &directory)?;
    let result = JsonValue::Object(BTreeMap::from([
        ("committed".to_owned(), JsonValue::Bool(committed)),
        (
            "error_code".to_owned(),
            JsonValue::String(error.code.to_owned()),
        ),
        (
            "error_sha256".to_owned(),
            JsonValue::String(crate::sha256_hex(error.message.as_bytes())),
        ),
        ("job_id".to_owned(), JsonValue::String(job_id.to_owned())),
        (
            "schema".to_owned(),
            JsonValue::String(RESULT_SCHEMA.to_owned()),
        ),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(spec_sha256.to_owned()),
        ),
        (
            "status".to_owned(),
            JsonValue::String(WorkerStatus::Failed.as_str().to_owned()),
        ),
    ]));
    atomic_write(
        &directory.join(RESULT_FILE),
        format!("{}\n", super::json::encode(&result)).as_bytes(),
    )
    .map(|_| ())
}

fn write_preflight_failure_receipt(
    root: &Path,
    job_id: &str,
    error: &JobError,
) -> Result<(), JobError> {
    let snapshot = super::status(root, job_id)?;
    let status = field_string(&snapshot, "status")?;
    if !matches!(status, "accepted" | "queued") {
        return Ok(());
    }
    let spec_sha256 = field_string(&snapshot, "spec_sha256")?;
    let directory = job_directory(root, job_id);
    ensure_job_contained(root, &directory)?;
    let result = JsonValue::Object(BTreeMap::from([
        ("committed".to_owned(), JsonValue::Bool(true)),
        (
            "error_code".to_owned(),
            JsonValue::String(error.code.to_owned()),
        ),
        (
            "error_sha256".to_owned(),
            JsonValue::String(crate::sha256_hex(error.message.as_bytes())),
        ),
        ("job_id".to_owned(), JsonValue::String(job_id.to_owned())),
        (
            "phase".to_owned(),
            JsonValue::String("preflight".to_owned()),
        ),
        (
            "schema".to_owned(),
            JsonValue::String(RESULT_SCHEMA.to_owned()),
        ),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(spec_sha256.to_owned()),
        ),
        (
            "status".to_owned(),
            JsonValue::String("preflight_failed".to_owned()),
        ),
    ]));
    atomic_write(
        &directory.join(RESULT_FILE),
        format!("{}\n", super::json::encode(&result)).as_bytes(),
    )
    .map(|_| ())
}

fn fail_starting(
    root: &Path,
    job_id: &str,
    lease_id: &str,
    _message: &str,
) -> Result<(), JobError> {
    executor_transition(root, job_id, lease_id, Status::Failed).map(|_| ())
}

fn field_string<'a>(value: &'a JsonValue, key: &str) -> Result<&'a str, JobError> {
    match value {
        JsonValue::Object(object) => match object.get(key) {
            Some(JsonValue::String(value)) => Ok(value),
            _ => Err(JobError::new(
                "SEL-J007",
                format!("worker result field {key} is missing"),
            )),
        },
        _ => Err(JobError::new("SEL-J007", "worker result is not an object")),
    }
}

fn field_u64(value: &JsonValue, key: &str) -> Result<u64, JobError> {
    match value {
        JsonValue::Object(object) => match object.get(key) {
            Some(JsonValue::Number(value)) => value.parse().map_err(|_| {
                JobError::new("SEL-J007", format!("worker field {key} is not an integer"))
            }),
            _ => Err(JobError::new(
                "SEL-J007",
                format!("worker field {key} is missing"),
            )),
        },
        _ => Err(JobError::new("SEL-J007", "worker result is not an object")),
    }
}

impl Capture {
    fn empty() -> Self {
        Self {
            bytes: 0,
            sha256: crate::sha256_hex(&[]),
            truncated: false,
        }
    }

    fn from_error(error: &JobError) -> Self {
        let bytes = error.message.as_bytes();
        Self {
            bytes: bytes.len(),
            sha256: crate::sha256_hex(bytes),
            truncated: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "seloryn-job-worker-{name}-{}-{}",
            std::process::id(),
            super::super::unix_nanos()
        ));
        fs::create_dir_all(&root).expect("temp root");
        root
    }

    #[test]
    fn rejects_non_seloryn_binary() {
        let error =
            resolve_binary(Some(Path::new("not-a-seloryn-binary"))).expect_err("must reject");
        assert_eq!(error.code, "SEL-J023");
    }

    #[test]
    fn rejects_zero_timeout_and_lease_overflow() {
        assert_eq!(lease_seconds(0).expect_err("zero timeout").code, "SEL-J022");
        assert_eq!(lease_seconds(3_600_000).expect("one-hour bound"), 3600);
    }

    #[test]
    fn preflight_rejects_path_escape_before_claim() {
        let root = temp_root("path");
        let spec = root.join("spec.json");
        fs::write(
            &spec,
            "{\"kind\":\"seloryn-run.v1\",\"source\":\"../escape.sely\",\"source_sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"timeout_ms\":1000}",
        )
        .expect("spec");
        super::super::create(&root, &spec, Some(std::ffi::OsStr::new("escape"))).expect("create");
        let error = preflight(&root, "escape").expect_err("path escape");
        assert_eq!(error.code, "SEL-J021");
        let state = super::super::status(&root, "escape").expect("status");
        assert!(
            matches!(state, JsonValue::Object(ref object) if object.get("status") == Some(&JsonValue::String("accepted".to_owned())))
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worker_input_rejects_wrong_lease() {
        let root = temp_root("lease");
        let source = root.join("main.sely");
        fs::write(&source, "fn main() -> Unit:\n    return\n").expect("source");
        let source_hash = crate::sha256_hex(&fs::read(&source).expect("read source"));
        let spec = root.join("spec.json");
        fs::write(
            &spec,
            format!(
                "{{\"kind\":\"seloryn-run.v1\",\"source\":\"main.sely\",\"source_sha256\":\"{source_hash}\",\"timeout_ms\":1000}}"
            ),
        )
        .expect("spec");
        super::super::create(&root, &spec, Some(std::ffi::OsStr::new("lease"))).expect("create");
        let claimed = super::super::claim(
            &root,
            "lease",
            Some(std::ffi::OsStr::new("worker")),
            None,
            5,
        )
        .expect("claim");
        let lease = field_string(&claimed, "lease_id").expect("lease id");
        let error = worker_input(&root, "lease", "wrong").expect_err("wrong lease");
        assert_eq!(error.code, "SEL-J017");
        let input = worker_input(&root, "lease", lease).expect("snapshot input");
        assert_eq!(
            fs::read(&input.source).expect("snapshot bytes"),
            b"fn main() -> Unit:\n    return\n"
        );
        fs::write(&source, "fn main() -> Unit:\n    return\n\n").expect("mutate source");
        assert_eq!(
            fs::read(&input.source).expect("stable snapshot bytes"),
            b"fn main() -> Unit:\n    return\n"
        );
        assert_eq!(
            executor_transition(&root, "lease", "wrong", Status::Running)
                .expect_err("wrong executor lease")
                .code,
            "SEL-J017"
        );
        executor_transition(&root, "lease", lease, Status::Running).expect("running");
        heartbeat(&root, "lease", lease, 5).expect("heartbeat while running");
        executor_transition(&root, "lease", lease, Status::Committing).expect("committing");
        executor_transition(&root, "lease", lease, Status::Failed).expect("failed");
        let state = super::super::status(&root, "lease").expect("final status");
        assert!(
            matches!(state, JsonValue::Object(ref object) if object.get("status") == Some(&JsonValue::String("failed".to_owned())))
        );
        let _ = fs::remove_dir_all(root);
    }
}
