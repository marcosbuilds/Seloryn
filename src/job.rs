use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "job_worker.rs"]
pub(crate) mod job_worker;

use crate::json::{self, JsonValue};

const JOB_SCHEMA: &str = "seloryn.job.v1";
const EVENT_SCHEMA: &str = "seloryn.job-event.v1";
const COMMIT_SCHEMA: &str = "seloryn.job-commit.v1";
const COMMIT_MARKER: &str = ".job-commit.json";
const COMMIT_DIRECTORY_PREFIX: &str = ".job-commit-tmp-";
const MAX_JOB_ID_BYTES: usize = 64;
const MAX_SPEC_BYTES: usize = 4 * 1024 * 1024;
const MAX_PREFLIGHT_SOURCE_BYTES: usize = 16 * 1024 * 1024;
#[allow(dead_code)]
const MAX_RESULT_BYTES: usize = 1024 * 1024;
const MAX_JSON_DEPTH: usize = 64;
const LOCK_WAIT_ATTEMPTS: usize = 500;
const LOCK_WAIT_STEP: Duration = Duration::from_millis(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Status {
    Accepted,
    Queued,
    Starting,
    Running,
    Committing,
    Succeeded,
    Cancelling,
    Cancelled,
    Failed,
    Lost,
    RetryWait,
}

impl Status {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Queued => "queued",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Committing => "committing",
            Self::Succeeded => "succeeded",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Lost => "lost",
            Self::RetryWait => "retry_wait",
        }
    }

    fn parse(value: &str) -> Result<Self, JobError> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "queued" => Ok(Self::Queued),
            "starting" => Ok(Self::Starting),
            "running" => Ok(Self::Running),
            "committing" => Ok(Self::Committing),
            "succeeded" => Ok(Self::Succeeded),
            "cancelling" => Ok(Self::Cancelling),
            "cancelled" => Ok(Self::Cancelled),
            "failed" => Ok(Self::Failed),
            "lost" => Ok(Self::Lost),
            "retry_wait" => Ok(Self::RetryWait),
            _ => Err(JobError::new(
                "SEL-J006",
                format!("unknown job status {value}"),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct JobState {
    job_id: String,
    status: Status,
    sequence: u64,
    attempt: u64,
    spec_sha256: String,
    created_at: u64,
    updated_at: u64,
    owner: Option<String>,
    attempt_id: Option<String>,
    claimed_at: Option<u64>,
    lease_id: Option<String>,
    lease_expires_at: Option<u64>,
}

#[derive(Clone, Debug)]
struct Event {
    sequence: u64,
    job_id: String,
    from: Option<Status>,
    to: Status,
    at: u64,
    spec_sha256: String,
    attempt: Option<u64>,
    kind: String,
    owner: Option<String>,
    attempt_id: Option<String>,
    claimed_at: Option<u64>,
    lease_id: Option<String>,
    lease_expires_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JobError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl JobError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn io(code: &'static str, context: &str, error: io::Error) -> Self {
        Self::new(code, format!("{context}: {error}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation {
    Create,
    Status,
    Transition,
    Retry,
    Claim,
    Heartbeat,
    Release,
    Recover,
    Preflight,
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Status => "status",
            Self::Transition => "transition",
            Self::Retry => "retry",
            Self::Claim => "claim",
            Self::Heartbeat => "heartbeat",
            Self::Release => "release",
            Self::Recover => "recover",
            Self::Preflight => "preflight",
        }
    }
}

pub(crate) fn create(
    root: &Path,
    spec_path: &Path,
    requested_id: Option<&OsStr>,
) -> Result<JsonValue, JobError> {
    let spec_bytes = fs::read(spec_path)
        .map_err(|error| JobError::io("SEL-J001", "failed to read job spec", error))?;
    if spec_bytes.len() > MAX_SPEC_BYTES {
        return Err(JobError::new(
            "SEL-J002",
            format!("job spec exceeds the {} byte limit", MAX_SPEC_BYTES),
        ));
    }
    let source = String::from_utf8(spec_bytes)
        .map_err(|_| JobError::new("SEL-J002", "job spec must be valid UTF-8 JSON"))?;
    let value = json::parse(&source, MAX_SPEC_BYTES, MAX_JSON_DEPTH).map_err(|error| {
        JobError::new("SEL-J002", format!("invalid job spec: {}", error.message))
    })?;
    let canonical_spec = format!("{}\n", json::encode(&value));
    let spec_sha256 = crate::sha256_hex(canonical_spec.as_bytes());
    let job_id = match requested_id {
        Some(value) => {
            let value = value
                .to_str()
                .ok_or_else(|| JobError::new("SEL-J003", "job id must be valid UTF-8"))?;
            validate_job_id(value)?;
            value.to_owned()
        }
        None => generated_job_id(),
    };

    let directory = job_directory(root, &job_id);
    let jobs_root = directory
        .parent()
        .expect("job directory has a parent")
        .to_path_buf();
    ensure_root_directory(root)?;
    ensure_directory(&root.join(".seloryn"), "job metadata directory")?;
    ensure_directory(&jobs_root, "job store")?;
    ensure_store_contained(root, &jobs_root)?;
    let now = unix_seconds();
    let mut reused = false;
    let temporary = jobs_root.join(format!(
        ".{job_id}.tmp-{}-{}",
        std::process::id(),
        unix_nanos()
    ));
    let state = JobState {
        job_id: job_id.clone(),
        status: Status::Accepted,
        sequence: 0,
        attempt: 1,
        spec_sha256: spec_sha256.clone(),
        created_at: now,
        updated_at: now,
        owner: None,
        attempt_id: None,
        claimed_at: None,
        lease_id: None,
        lease_expires_at: None,
    };
    let event = Event {
        sequence: 0,
        job_id: job_id.clone(),
        from: None,
        to: Status::Accepted,
        at: now,
        spec_sha256: spec_sha256.clone(),
        attempt: Some(1),
        kind: "create".to_owned(),
        owner: None,
        attempt_id: None,
        claimed_at: None,
        lease_id: None,
        lease_expires_at: None,
    };
    match fs::create_dir(&temporary) {
        Ok(()) => {
            let write_result = (|| -> Result<(), JobError> {
                atomic_write(&temporary.join("spec.json"), canonical_spec.as_bytes())?;
                atomic_write(
                    &temporary.join("events.jsonl"),
                    event_line(&event).as_bytes(),
                )?;
                atomic_write(&temporary.join("state.json"), state_json(&state).as_bytes())?;
                Ok(())
            })();
            if let Err(error) = write_result {
                let _ = fs::remove_dir_all(&temporary);
                return Err(error);
            }
            match fs::rename(&temporary, &directory) {
                Ok(()) => sync_directory(&jobs_root),
                Err(error) if is_existing_directory_error(&error) => {
                    let _ = fs::remove_dir_all(&temporary);
                    ensure_job_contained(root, &directory)?;
                    let record = load_record(&directory)?;
                    if record.state.spec_sha256 != spec_sha256 {
                        return Err(JobError::new(
                            "SEL-J004",
                            format!("job {job_id} already exists with a different spec"),
                        ));
                    }
                    reused = true;
                }
                Err(error) => {
                    let _ = fs::remove_dir_all(&temporary);
                    return Err(JobError::io(
                        "SEL-J010",
                        "failed to publish the job directory",
                        error,
                    ));
                }
            }
        }
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to create the temporary job directory",
                error,
            ));
        }
    }

    ensure_job_contained(root, &directory)?;
    let record = load_record(&directory)?;
    Ok(result_json(Operation::Create, &record, Some(reused), None))
}

pub(crate) fn status(root: &Path, job_id: &str) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let record = load_record(&directory)?;
    Ok(result_json(Operation::Status, &record, None, None))
}

pub(crate) fn preflight(root: &Path, job_id: &str) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let record = load_record_unlocked_read_only_without_spec(&directory)?;
    if !matches!(record.state.status, Status::Accepted | Status::Queued) {
        return Err(preflight_error(
            "job must be in accepted or queued state for preflight",
        ));
    }

    let input = read_preflight_input(root, &directory, &record.state.spec_sha256)?;

    Ok(JsonValue::Object(BTreeMap::from([
        (
            "attempt".to_owned(),
            JsonValue::Number(record.state.attempt.to_string()),
        ),
        ("job_id".to_owned(), JsonValue::String(record.state.job_id)),
        (
            "operation".to_owned(),
            JsonValue::String(Operation::Preflight.as_str().to_owned()),
        ),
        (
            "schema".to_owned(),
            JsonValue::String(JOB_SCHEMA.to_owned()),
        ),
        ("source".to_owned(), JsonValue::String(input.spec.source)),
        (
            "source_bytes".to_owned(),
            JsonValue::Number(input.source_bytes.len().to_string()),
        ),
        (
            "source_sha256".to_owned(),
            JsonValue::String(input.spec.source_sha256),
        ),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(record.state.spec_sha256),
        ),
        ("status".to_owned(), JsonValue::String("ready".to_owned())),
        (
            "timeout_ms".to_owned(),
            JsonValue::Number(input.spec.timeout_ms.to_string()),
        ),
    ])))
}

struct PreflightSpec {
    source: String,
    source_sha256: String,
    timeout_ms: u64,
}

fn parse_preflight_spec(
    bytes: &[u8],
    expected_spec_sha256: &str,
) -> Result<PreflightSpec, JobError> {
    if bytes.len() > MAX_SPEC_BYTES {
        return Err(preflight_error(format!(
            "job spec exceeds the {MAX_SPEC_BYTES} byte limit"
        )));
    }
    let source = String::from_utf8(bytes.to_vec())
        .map_err(|_| preflight_error("job spec must be valid UTF-8 JSON"))?;
    let value = json::parse(&source, MAX_SPEC_BYTES, MAX_JSON_DEPTH)
        .map_err(|error| preflight_error(format!("invalid job spec: {}", error.message)))?;
    let canonical_spec = format!("{}\n", json::encode(&value));
    let actual_spec_sha256 = crate::sha256_hex(canonical_spec.as_bytes());
    if actual_spec_sha256 != expected_spec_sha256 {
        return Err(preflight_error(
            "job spec digest does not match the record spec_sha256",
        ));
    }
    let object = object(value, "job spec").map_err(|error| preflight_error(error.message))?;
    let expected = ["kind", "source", "source_sha256", "timeout_ms"];
    if object.len() != expected.len() || expected.iter().any(|key| !object.contains_key(*key)) {
        return Err(preflight_error(
            "job spec must contain exactly kind, source, source_sha256 and timeout_ms",
        ));
    }
    let kind =
        object_string_from(&object, "kind").map_err(|error| preflight_error(error.message))?;
    if kind != "seloryn-run.v1" {
        return Err(preflight_error("job spec kind must be seloryn-run.v1"));
    }
    let source = object_string_from(&object, "source")
        .map_err(|error| preflight_error(error.message))?
        .to_owned();
    let source_sha256 = object_string_from(&object, "source_sha256")
        .map_err(|error| preflight_error(error.message))?
        .to_owned();
    if source_sha256.len() != 64
        || !source_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(preflight_error(
            "source_sha256 must be 64 lowercase hexadecimal characters",
        ));
    }
    let timeout_ms =
        object_u64(&object, "timeout_ms").map_err(|error| preflight_error(error.message))?;
    if !(1..=3_600_000).contains(&timeout_ms) {
        return Err(preflight_error("timeout_ms must be between 1 and 3600000"));
    }
    Ok(PreflightSpec {
        source,
        source_sha256,
        timeout_ms,
    })
}

struct ResolvedSource {
    path: PathBuf,
    canonical: PathBuf,
    canonical_root: PathBuf,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct WorkerInput {
    pub(crate) source: PathBuf,
    pub(crate) timeout: Duration,
    pub(crate) spec_sha256: String,
    pub(crate) attempt: u64,
}

fn read_preflight_input(
    root: &Path,
    directory: &Path,
    expected_spec_sha256: &str,
) -> Result<PreflightInput, JobError> {
    let spec_path = directory.join("spec.json");
    reject_symlink(&spec_path)?;
    let spec_bytes = fs::read(&spec_path)
        .map_err(|error| JobError::io("SEL-J010", "failed to read job spec", error))?;
    let spec = parse_preflight_spec(&spec_bytes, expected_spec_sha256)?;
    let resolved_source = resolve_preflight_source(root, &spec.source)?;
    let mut source_file = File::open(&resolved_source.path)
        .map_err(|error| JobError::io("SEL-J010", "failed to open job source", error))?;
    let source_metadata = source_file
        .metadata()
        .map_err(|error| JobError::io("SEL-J010", "failed to inspect job source", error))?;
    if metadata_is_reparse(&source_metadata) || !source_metadata.is_file() {
        return Err(preflight_error("source is not a regular file"));
    }
    if source_metadata.len() > MAX_PREFLIGHT_SOURCE_BYTES as u64 {
        return Err(preflight_error(format!(
            "source exceeds the {MAX_PREFLIGHT_SOURCE_BYTES} byte limit"
        )));
    }
    let mut source_bytes = Vec::new();
    std::io::Read::by_ref(&mut source_file)
        .take(MAX_PREFLIGHT_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut source_bytes)
        .map_err(|error| JobError::io("SEL-J010", "failed to read job source", error))?;
    if source_bytes.len() > MAX_PREFLIGHT_SOURCE_BYTES {
        return Err(preflight_error(format!(
            "source exceeds the {MAX_PREFLIGHT_SOURCE_BYTES} byte limit"
        )));
    }
    let canonical_after_read = fs::canonicalize(&resolved_source.path)
        .map_err(|error| JobError::io("SEL-J010", "failed to re-resolve the source path", error))?;
    if canonical_after_read != resolved_source.canonical
        || !canonical_after_read.starts_with(&resolved_source.canonical_root)
    {
        return Err(preflight_error(
            "source path changed or resolves outside the selected root",
        ));
    }
    let source = String::from_utf8(source_bytes.clone())
        .map_err(|_| preflight_error("source must be valid UTF-8"))?;
    let source_sha256 = crate::sha256_hex(&source_bytes);
    if source_sha256 != spec.source_sha256 {
        return Err(preflight_error(
            "source_sha256 does not match source content",
        ));
    }
    let parsed = crate::parser::parse(&source);
    if let Some(diagnostic) = parsed.diagnostics.first() {
        return Err(source_diagnostic_error(diagnostic));
    }
    let program = parsed
        .program
        .ok_or_else(|| preflight_error("source parser produced no program"))?;
    if let Some(diagnostic) = crate::semantic::check(&program).first() {
        return Err(source_diagnostic_error(diagnostic));
    }
    Ok(PreflightInput {
        spec,
        resolved_source,
        source_bytes,
    })
}

#[allow(dead_code)]
struct PreflightInput {
    spec: PreflightSpec,
    resolved_source: ResolvedSource,
    source_bytes: Vec<u8>,
}

#[allow(dead_code)]
pub(crate) fn worker_input(
    root: &Path,
    job_id: &str,
    lease_id: &str,
) -> Result<WorkerInput, JobError> {
    validate_job_id(job_id)?;
    validate_lease_id(lease_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let record = load_record_unlocked_read_only_without_spec(&directory)?;
    if record.state.status != Status::Starting {
        return Err(JobError::new(
            "SEL-J017",
            "worker input requires a starting job",
        ));
    }
    require_lease(&record.state, lease_id)?;
    if !lease_active(&record.state, unix_seconds()) {
        return Err(JobError::new("SEL-J016", "lease has expired"));
    }
    let input = read_preflight_input(root, &directory, &record.state.spec_sha256)?;
    // Execute a job-owned snapshot rather than reopening the user-controlled
    // source path after validation. This removes the relative-root ambiguity
    // and closes the preflight-to-spawn replacement window for the v1 worker.
    let snapshot = directory.join("source.sely");
    reject_symlink(&snapshot)?;
    atomic_write(&snapshot, &input.source_bytes)?;
    let snapshot_metadata = fs::symlink_metadata(&snapshot)
        .map_err(|error| JobError::io("SEL-J010", "failed to inspect source snapshot", error))?;
    if metadata_is_reparse(&snapshot_metadata) || !snapshot_metadata.is_file() {
        return Err(preflight_error("source snapshot is not a regular file"));
    }
    let snapshot = fs::canonicalize(&snapshot)
        .map_err(|error| JobError::io("SEL-J010", "failed to resolve source snapshot", error))?;
    Ok(WorkerInput {
        source: snapshot,
        timeout: Duration::from_millis(input.spec.timeout_ms),
        spec_sha256: record.state.spec_sha256,
        attempt: record.state.attempt,
    })
}

fn resolve_preflight_source(root: &Path, source: &str) -> Result<ResolvedSource, JobError> {
    if source.is_empty()
        || source.len() > 1024
        || !source.ends_with(".sely")
        || source.starts_with('/')
        || source.contains('\\')
        || source.as_bytes().get(1).is_some_and(|byte| *byte == b':')
        || source.chars().any(char::is_control)
        || source
            .chars()
            .any(|character| matches!(character, ':' | '*' | '?' | '"' | '<' | '>' | '|'))
    {
        return Err(preflight_error(
            "source must be a relative UTF-8 .sely path within 1024 bytes without forbidden syntax",
        ));
    }
    let components = source.split('/').collect::<Vec<_>>();
    if components.iter().any(|component| {
        component.is_empty()
            || component.len() > 255
            || matches!(*component, "." | "..")
            || component.ends_with('.')
            || component.ends_with(' ')
            || is_windows_reserved_component(component)
    }) {
        return Err(preflight_error("source path contains an invalid component"));
    }

    let root_metadata = fs::symlink_metadata(root)
        .map_err(|error| JobError::io("SEL-J010", "failed to inspect the job root", error))?;
    if metadata_is_reparse(&root_metadata) || !root_metadata.is_dir() {
        return Err(preflight_error("job root must be a real directory"));
    }
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| JobError::io("SEL-J010", "failed to resolve the job root", error))?;
    let mut path = root.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        path.push(component);
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == io::ErrorKind::NotFound {
                preflight_error("source file or directory component does not exist")
            } else {
                JobError::io("SEL-J010", "failed to inspect the source path", error)
            }
        })?;
        if metadata_is_reparse(&metadata) {
            return Err(preflight_error(
                "source path contains a symlink, junction or reparse point",
            ));
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err(preflight_error("source path component is not a directory"));
        }
        if index + 1 == components.len() && !metadata.is_file() {
            return Err(preflight_error("source is not a regular file"));
        }
    }
    let canonical_source = fs::canonicalize(&path)
        .map_err(|error| JobError::io("SEL-J010", "failed to resolve the source path", error))?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(preflight_error("source resolves outside the selected root"));
    }
    Ok(ResolvedSource {
        path,
        canonical: canonical_source,
        canonical_root,
    })
}

fn is_windows_reserved_component(component: &str) -> bool {
    let base = component.split('.').next().unwrap_or_default();
    let upper = base.to_ascii_uppercase();
    matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && upper.as_bytes()[3].is_ascii_digit()
            && upper.as_bytes()[3] != b'0')
}

fn preflight_error(message: impl Into<String>) -> JobError {
    JobError::new("SEL-J021", message)
}

fn source_diagnostic_error(diagnostic: &crate::diagnostic::Diagnostic) -> JobError {
    preflight_error(format!(
        "source diagnostic {}: {}",
        diagnostic.code, diagnostic.message
    ))
}

pub(crate) fn list(root: &Path) -> Result<JsonValue, JobError> {
    let jobs_root = jobs_directory(root);
    let mut jobs = Vec::new();
    let store_exists = match fs::symlink_metadata(&jobs_root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(JobError::new(
                "SEL-J012",
                "job store contains a symlink or reparse point",
            ));
        }
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => {
            return Err(JobError::new("SEL-J007", "job store is not a directory"));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to inspect the job store",
                error,
            ));
        }
    };
    if store_exists {
        ensure_store_contained(root, &jobs_root)?;
        let entries = fs::read_dir(&jobs_root)
            .map_err(|error| JobError::io("SEL-J010", "failed to list the job store", error))?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                JobError::io("SEL-J010", "failed to read a job directory entry", error)
            })?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|error| {
                JobError::io("SEL-J010", "failed to inspect a job directory entry", error)
            })?;
            if metadata_is_reparse(&metadata) {
                return Err(JobError::new(
                    "SEL-J012",
                    "job store contains a symlink or reparse point",
                ));
            }
            if !metadata.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(job_id) = name.to_str() else {
                return Err(JobError::new(
                    "SEL-J007",
                    "job store contains a non-UTF-8 directory name",
                ));
            };
            validate_job_id(job_id)?;
            ensure_job_contained(root, &path)?;
            let record = load_record(&path)?;
            jobs.push(job_summary(&record));
        }
    }
    jobs.sort_by(|left, right| {
        let left_id = object_string(left, "job_id").unwrap_or_default();
        let right_id = object_string(right, "job_id").unwrap_or_default();
        left_id.cmp(right_id)
    });
    Ok(JsonValue::Object(BTreeMap::from([
        (
            "schema".to_owned(),
            JsonValue::String("seloryn.job-list.v1".to_owned()),
        ),
        ("operation".to_owned(), JsonValue::String("list".to_owned())),
        ("jobs".to_owned(), JsonValue::Array(jobs)),
    ])))
}

pub(crate) fn transition(
    root: &Path,
    job_id: &str,
    next_status: &str,
) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    let next = Status::parse(next_status)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    if is_executor_owned(record.state.status) || is_executor_owned(next) {
        return Err(JobError::new(
            "SEL-J018",
            "public transition cannot fabricate or mutate executor-owned state",
        ));
    }
    if record.state.status == next {
        return Ok(result_json(
            Operation::Transition,
            &record,
            None,
            Some(next),
        ));
    }
    if record.state.status != Status::Accepted || next != Status::Queued {
        return Err(JobError::new(
            "SEL-J005",
            format!(
                "public transition {} -> {} is not an administrative transition",
                record.state.status.as_str(),
                next.as_str()
            ),
        ));
    }
    let now = unix_seconds();
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: next,
        at: now,
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(record.state.attempt),
        kind: "admin".to_owned(),
        owner: record.state.owner.clone(),
        attempt_id: record.state.attempt_id.clone(),
        claimed_at: record.state.claimed_at,
        lease_id: record.state.lease_id.clone(),
        lease_expires_at: record.state.lease_expires_at,
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(result_json(
        Operation::Transition,
        &record,
        None,
        Some(next),
    ))
}

/// Move one terminal attempt back to the queue without changing its
/// canonical spec. The old result is archived before the append-only retry
/// event is committed, so a retry cannot erase the evidence that motivated it.
pub(crate) fn retry(root: &Path, job_id: &str) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    if !matches!(
        record.state.status,
        Status::Lost | Status::Cancelled | Status::Failed
    ) {
        return Err(JobError::new(
            "SEL-J015",
            format!(
                "job {job_id} can only be retried from lost, cancelled or failed state, not {}",
                record.state.status.as_str()
            ),
        ));
    }
    archive_result(&directory, record.state.attempt)?;
    let result_path = directory.join("result.json");
    match fs::remove_file(&result_path) {
        Ok(()) => sync_directory(&directory),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to clear the active job result",
                error,
            ));
        }
    }
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: Status::Queued,
        at: unix_seconds(),
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(record.state.attempt),
        kind: "retry".to_owned(),
        owner: None,
        attempt_id: None,
        claimed_at: None,
        lease_id: None,
        lease_expires_at: None,
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(result_json(
        Operation::Retry,
        &record,
        Some(false),
        Some(Status::Queued),
    ))
}

pub(crate) fn claim(
    root: &Path,
    job_id: &str,
    requested_owner: Option<&OsStr>,
    requested_lease_id: Option<&OsStr>,
    ttl_seconds: u64,
) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    validate_ttl(ttl_seconds)?;
    let requested_owner = match requested_owner {
        Some(value) => Some(
            value
                .to_str()
                .ok_or_else(|| JobError::new("SEL-J014", "lease owner must be valid UTF-8"))?
                .to_owned(),
        ),
        None => None,
    };
    if let Some(owner) = &requested_owner {
        validate_identity(owner, "lease owner")?;
    }
    let requested_lease_id = match requested_lease_id {
        Some(value) => {
            let value = value
                .to_str()
                .ok_or_else(|| JobError::new("SEL-J014", "lease id must be valid UTF-8"))?;
            validate_lease_id(value)?;
            Some(value.to_owned())
        }
        None => None,
    };
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    match record.state.status {
        Status::Accepted | Status::Queued => {
            if record.state.owner.is_some()
                || record.state.attempt_id.is_some()
                || record.state.claimed_at.is_some()
                || record.state.lease_id.is_some()
                || record.state.lease_expires_at.is_some()
            {
                return Err(JobError::new(
                    "SEL-J019",
                    "queued job has inconsistent lease identity",
                ));
            }
        }
        Status::Starting => {
            if requested_lease_id.is_some()
                && requested_lease_id.as_deref() == record.state.lease_id.as_deref()
            {
                if !lease_active(&record.state, unix_seconds()) {
                    return Err(JobError::new(
                        "SEL-J016",
                        format!("job {job_id} lease expired; recover it explicitly"),
                    ));
                }
                if let Some(requested_owner) = requested_owner.as_deref()
                    && record.state.owner.as_deref() != Some(requested_owner)
                {
                    return Err(JobError::new(
                        "SEL-J017",
                        "lease owner does not match the bearer lease",
                    ));
                }
                return Ok(claim_result(&record, true));
            }
            if record.state.lease_id.is_some() && !lease_active(&record.state, unix_seconds()) {
                return Err(JobError::new(
                    "SEL-J016",
                    format!("job {job_id} lease expired; recover it explicitly"),
                ));
            }
            return Err(JobError::new(
                "SEL-J014",
                format!(
                    "job {job_id} is busy/in_progress under owner {}",
                    record.state.owner.as_deref().unwrap_or("unknown")
                ),
            ));
        }
        Status::Running => {
            return Err(JobError::new(
                "SEL-J014",
                format!("job {job_id} is busy/in_progress"),
            ));
        }
        Status::Lost => {
            return Err(JobError::new(
                "SEL-J015",
                format!("job {job_id} is terminal/lost; create a new job"),
            ));
        }
        status => {
            return Err(JobError::new(
                "SEL-J013",
                format!("job {job_id} cannot be claimed from {}", status.as_str()),
            ));
        }
    }
    let owner = requested_owner.unwrap_or_else(generated_owner);
    validate_identity(&owner, "lease owner")?;
    if requested_lease_id.is_some() {
        return Err(JobError::new(
            "SEL-J014",
            "a lease id can only retry the active bearer lease",
        ));
    }
    let events_source = read_required(&directory.join("events.jsonl"), "job event log")?;
    let events = parse_events(&events_source)?;
    let attempt = if events.iter().any(|event| event.to == Status::Starting) {
        record
            .state
            .attempt
            .checked_add(1)
            .ok_or_else(|| JobError::new("SEL-J019", "job attempt counter overflowed"))?
    } else {
        record.state.attempt
    };
    if attempt == 0 {
        return Err(JobError::new("SEL-J019", "job attempt must be positive"));
    }
    let now = unix_seconds();
    let attempt_id = generated_attempt_id(&owner);
    let lease_id = generated_lease_id(&owner, &attempt_id);
    let lease_expires_at = lease_deadline(ttl_seconds);
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: Status::Starting,
        at: now,
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(attempt),
        kind: "claim".to_owned(),
        owner: Some(owner.clone()),
        attempt_id: Some(attempt_id.clone()),
        claimed_at: Some(now),
        lease_id: Some(lease_id.clone()),
        lease_expires_at: Some(lease_expires_at),
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(claim_result(&record, false))
}

fn claim_result(record: &Record, idempotent: bool) -> JsonValue {
    let mut result = match result_json(Operation::Claim, record, None, Some(Status::Starting)) {
        JsonValue::Object(value) => value,
        _ => unreachable!(),
    };
    result.insert("lease".to_owned(), JsonValue::String("held".to_owned()));
    if idempotent {
        result.insert("idempotent".to_owned(), JsonValue::Bool(true));
    }
    JsonValue::Object(result)
}

pub(crate) fn heartbeat(
    root: &Path,
    job_id: &str,
    lease_id: &str,
    ttl_seconds: u64,
) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    validate_lease_id(lease_id)?;
    validate_ttl(ttl_seconds)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    if !matches!(
        record.state.status,
        Status::Starting | Status::Running | Status::Committing | Status::Cancelling
    ) {
        return Err(JobError::new(
            "SEL-J017",
            "heartbeat is only valid while the executor lease is active",
        ));
    }
    require_lease(&record.state, lease_id)?;
    let now = unix_seconds();
    if !lease_active(&record.state, now) {
        return Err(JobError::new("SEL-J016", "lease has expired"));
    }
    let current_expiry = record
        .state
        .lease_expires_at
        .ok_or_else(|| JobError::new("SEL-J019", "executor job has no lease expiry"))?;
    let lease_expires_at = lease_deadline(ttl_seconds).max(current_expiry.saturating_add(1));
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: record.state.status,
        at: now,
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(record.state.attempt),
        kind: "heartbeat".to_owned(),
        owner: record.state.owner.clone(),
        attempt_id: record.state.attempt_id.clone(),
        claimed_at: record.state.claimed_at,
        lease_id: Some(lease_id.to_owned()),
        lease_expires_at: Some(lease_expires_at),
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(result_json(
        Operation::Heartbeat,
        &record,
        None,
        Some(record.state.status),
    ))
}

/// Commit an executor-owned transition. This is deliberately not wired to the
/// public `transition` API: only a holder of the current lease can call it.
#[allow(dead_code)]
pub(crate) fn executor_transition(
    root: &Path,
    job_id: &str,
    lease_id: &str,
    next: Status,
) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    validate_lease_id(lease_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    if !matches!(
        record.state.status,
        Status::Starting | Status::Running | Status::Committing | Status::Cancelling
    ) {
        return Err(JobError::new(
            "SEL-J017",
            "executor transition requires an active executor state",
        ));
    }
    require_lease(&record.state, lease_id)?;
    if !lease_active(&record.state, unix_seconds()) {
        return Err(JobError::new("SEL-J016", "lease has expired"));
    }
    validate_transition(record.state.status, next)?;
    let terminal = matches!(next, Status::Succeeded | Status::Cancelled | Status::Failed);
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: next,
        at: unix_seconds(),
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(record.state.attempt),
        kind: "executor".to_owned(),
        owner: (!terminal).then(|| record.state.owner.clone()).flatten(),
        attempt_id: (!terminal)
            .then(|| record.state.attempt_id.clone())
            .flatten(),
        claimed_at: (!terminal).then_some(record.state.claimed_at).flatten(),
        lease_id: (!terminal).then(|| Some(lease_id.to_owned())).flatten(),
        lease_expires_at: (!terminal)
            .then_some(record.state.lease_expires_at)
            .flatten(),
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(result_json(
        Operation::Transition,
        &record,
        None,
        Some(next),
    ))
}

pub(crate) fn release(root: &Path, job_id: &str, lease_id: &str) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    validate_lease_id(lease_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    if record.state.status != Status::Starting {
        return Err(JobError::new(
            "SEL-J017",
            "release is only valid while the lease is starting",
        ));
    }
    require_lease(&record.state, lease_id)?;
    if !lease_active(&record.state, unix_seconds()) {
        return Err(JobError::new("SEL-J016", "lease has expired"));
    }
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(Status::Starting),
        to: Status::Queued,
        at: unix_seconds(),
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(record.state.attempt),
        kind: "release".to_owned(),
        owner: None,
        attempt_id: None,
        claimed_at: None,
        lease_id: None,
        lease_expires_at: None,
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(result_json(
        Operation::Release,
        &record,
        None,
        Some(Status::Queued),
    ))
}

pub(crate) fn recover_lost(root: &Path, job_id: &str) -> Result<JsonValue, JobError> {
    validate_job_id(job_id)?;
    let directory = job_directory(root, job_id);
    ensure_store_contained(
        root,
        directory.parent().expect("job directory has a parent"),
    )?;
    ensure_job_contained(root, &directory)?;
    let _lock = JobLock::acquire(&directory)?;
    let mut record = load_record_unlocked(&directory)?;
    if record.state.status == Status::Lost {
        return Ok(result_json(Operation::Recover, &record, Some(true), None));
    }
    if !matches!(
        record.state.status,
        Status::Starting | Status::Running | Status::Committing | Status::Cancelling
    ) {
        return Err(JobError::new(
            "SEL-J015",
            format!(
                "job {job_id} can only be recovered from an active executor state, not {}",
                record.state.status.as_str()
            ),
        ));
    }
    let now = unix_seconds();
    if record.state.lease_id.is_none() || record.state.lease_expires_at.is_none() {
        return Err(JobError::new(
            "SEL-J019",
            "legacy executor job has no lease; automatic recovery is refused",
        ));
    }
    if lease_active(&record.state, now) {
        return Err(JobError::new(
            "SEL-J016",
            format!("job {job_id} lease is still active"),
        ));
    }
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: Status::Lost,
        at: now,
        spec_sha256: record.state.spec_sha256.clone(),
        attempt: Some(record.state.attempt),
        kind: "recover".to_owned(),
        owner: record.state.owner.clone(),
        attempt_id: record.state.attempt_id.clone(),
        claimed_at: record.state.claimed_at,
        lease_id: None,
        lease_expires_at: None,
    };
    let record = commit_event(&directory, &mut record, event)?;
    Ok(result_json(
        Operation::Recover,
        &record,
        Some(false),
        Some(Status::Lost),
    ))
}

fn commit_event(directory: &Path, record: &mut Record, event: Event) -> Result<Record, JobError> {
    recover_pending_commit(directory)?;
    let event_path = directory.join("events.jsonl");
    let events = fs::read(&event_path)
        .map_err(|error| JobError::io("SEL-J007", "failed to read the job event log", error))?;
    if !events.ends_with(b"\n") {
        return Err(JobError::new(
            "SEL-J008",
            "job event log does not end at a complete line",
        ));
    }
    let stored_state = fs::read(directory.join("state.json"))
        .map_err(|error| JobError::io("SEL-J007", "failed to read job state", error))?;
    let expected_sequence = record
        .state
        .sequence
        .checked_add(1)
        .ok_or_else(|| JobError::new("SEL-J019", "job sequence counter overflowed"))?;
    if event.sequence != expected_sequence {
        return Err(JobError::new(
            "SEL-J007",
            "job event sequence does not follow the current state",
        ));
    }

    let mut next_state = record.state.clone();
    apply_event(&mut next_state, &event);
    let mut next_events = events.clone();
    next_events.extend_from_slice(event_line(&event).as_bytes());
    let pending = PendingCommit {
        temp_directory: format!(
            "{COMMIT_DIRECTORY_PREFIX}{}-{}",
            std::process::id(),
            unix_nanos()
        ),
        base_events_sha256: crate::sha256_hex(&events),
        base_state_sha256: crate::sha256_hex(&stored_state),
        events_sha256: crate::sha256_hex(&next_events),
        state_sha256: crate::sha256_hex(state_json(&next_state).as_bytes()),
        sequence: event.sequence,
    };
    let temporary = directory.join(&pending.temp_directory);
    fs::create_dir(&temporary).map_err(|error| {
        JobError::io(
            "SEL-J010",
            "failed to create the job commit transaction",
            error,
        )
    })?;
    let prepare_result = (|| -> Result<(), JobError> {
        atomic_write(&temporary.join("events.jsonl"), &next_events)?;
        atomic_write(
            &temporary.join("state.json"),
            state_json(&next_state).as_bytes(),
        )?;
        atomic_write(
            &directory.join(COMMIT_MARKER),
            pending_commit_json(&pending).as_bytes(),
        )?;
        Ok(())
    })();
    if let Err(error) = prepare_result {
        let _ = fs::remove_file(directory.join(COMMIT_MARKER));
        let _ = remove_commit_directory(&temporary);
        return Err(error);
    }

    recover_pending_commit(directory)?;
    load_record_unlocked(directory)
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingCommit {
    temp_directory: String,
    base_events_sha256: String,
    base_state_sha256: String,
    events_sha256: String,
    state_sha256: String,
    sequence: u64,
}

fn apply_event(state: &mut JobState, event: &Event) {
    state.status = event.to;
    state.sequence = event.sequence;
    state.updated_at = event.at;
    state.attempt = event.attempt.unwrap_or(state.attempt);
    state.owner = event.owner.clone();
    state.attempt_id = event.attempt_id.clone();
    state.claimed_at = event.claimed_at;
    state.lease_id = event.lease_id.clone();
    state.lease_expires_at = event.lease_expires_at;
}

fn pending_commit_json(commit: &PendingCommit) -> String {
    json::encode(&JsonValue::Object(BTreeMap::from([
        (
            "base_events_sha256".to_owned(),
            JsonValue::String(commit.base_events_sha256.clone()),
        ),
        (
            "base_state_sha256".to_owned(),
            JsonValue::String(commit.base_state_sha256.clone()),
        ),
        (
            "events_sha256".to_owned(),
            JsonValue::String(commit.events_sha256.clone()),
        ),
        (
            "schema".to_owned(),
            JsonValue::String(COMMIT_SCHEMA.to_owned()),
        ),
        (
            "sequence".to_owned(),
            JsonValue::Number(commit.sequence.to_string()),
        ),
        (
            "state_sha256".to_owned(),
            JsonValue::String(commit.state_sha256.clone()),
        ),
        (
            "temp_directory".to_owned(),
            JsonValue::String(commit.temp_directory.clone()),
        ),
    ]))) + "\n"
}

fn parse_pending_commit(source: &str) -> Result<PendingCommit, JobError> {
    let value = json::parse(source, 16 * 1024, 8).map_err(|error| {
        JobError::new(
            "SEL-J007",
            format!("job commit transaction is invalid: {}", error.message),
        )
    })?;
    let object = object(value, "job commit transaction")?;
    if object_string_from(&object, "schema")? != COMMIT_SCHEMA {
        return Err(JobError::new(
            "SEL-J007",
            "job commit transaction schema is unsupported",
        ));
    }
    let temp_directory = object_string_from(&object, "temp_directory")?.to_owned();
    let Some(temp_suffix) = temp_directory.strip_prefix(COMMIT_DIRECTORY_PREFIX) else {
        return Err(JobError::new(
            "SEL-J007",
            "job commit transaction directory is invalid",
        ));
    };
    if temp_suffix.is_empty()
        || temp_directory.len() > 128
        || temp_suffix.chars().any(|character| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        })
    {
        return Err(JobError::new(
            "SEL-J007",
            "job commit transaction directory is invalid",
        ));
    }
    Ok(PendingCommit {
        temp_directory,
        base_events_sha256: object_string_from(&object, "base_events_sha256")?.to_owned(),
        base_state_sha256: object_string_from(&object, "base_state_sha256")?.to_owned(),
        events_sha256: object_string_from(&object, "events_sha256")?.to_owned(),
        state_sha256: object_string_from(&object, "state_sha256")?.to_owned(),
        sequence: object_u64(&object, "sequence")?,
    })
}

fn recover_pending_commit(directory: &Path) -> Result<(), JobError> {
    let marker_path = directory.join(COMMIT_MARKER);
    let marker_source = match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => {
            if metadata_is_reparse(&metadata) {
                return Err(JobError::new(
                    "SEL-J012",
                    "job store contains a symlink or reparse point",
                ));
            }
            read_required(&marker_path, "job commit transaction")?
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to inspect the job commit transaction",
                error,
            ));
        }
    };
    let pending = parse_pending_commit(&marker_source)?;
    let temporary = directory.join(&pending.temp_directory);
    let temporary_exists = match fs::symlink_metadata(&temporary) {
        Ok(metadata) => {
            if metadata_is_reparse(&metadata) || !metadata.is_dir() {
                return Err(JobError::new(
                    "SEL-J012",
                    "job commit transaction contains a symlink or reparse point",
                ));
            }
            true
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to inspect the job commit transaction directory",
                error,
            ));
        }
    };
    let event_path = directory.join("events.jsonl");
    let state_path = directory.join("state.json");
    reject_symlink(&event_path)?;
    reject_symlink(&state_path)?;
    let current_events = fs::read(&event_path)
        .map_err(|error| JobError::io("SEL-J007", "failed to read the job event log", error))?;
    let current_state = fs::read(&state_path)
        .map_err(|error| JobError::io("SEL-J007", "failed to read job state", error))?;
    let current_events_sha256 = crate::sha256_hex(&current_events);
    let current_state_sha256 = crate::sha256_hex(&current_state);
    let current_is_base = current_events_sha256 == pending.base_events_sha256
        && current_state_sha256 == pending.base_state_sha256;
    let current_is_committed = current_events_sha256 == pending.events_sha256
        && current_state_sha256 == pending.state_sha256;
    if !current_is_base && !current_is_committed {
        let event_matches = current_events_sha256 == pending.events_sha256
            || current_events_sha256 == pending.base_events_sha256;
        let state_matches = current_state_sha256 == pending.state_sha256
            || current_state_sha256 == pending.base_state_sha256;
        if !event_matches || !state_matches {
            return Err(JobError::new(
                "SEL-J007",
                "job commit transaction conflicts with current event/state files",
            ));
        }
    }

    let (candidate_events, candidate_state) = if temporary_exists {
        let temporary_events_path = temporary.join("events.jsonl");
        let temporary_state_path = temporary.join("state.json");
        reject_symlink(&temporary_events_path)?;
        reject_symlink(&temporary_state_path)?;
        let events = fs::read(temporary_events_path).map_err(|error| {
            JobError::io("SEL-J010", "failed to read the pending event log", error)
        })?;
        let state = fs::read(temporary_state_path)
            .map_err(|error| JobError::io("SEL-J010", "failed to read pending job state", error))?;
        if crate::sha256_hex(&events) != pending.events_sha256
            || crate::sha256_hex(&state) != pending.state_sha256
        {
            return Err(JobError::new(
                "SEL-J007",
                "job commit transaction candidate hashes do not match",
            ));
        }
        parse_events(
            std::str::from_utf8(&events).map_err(|_| {
                JobError::new("SEL-J007", "pending job event log is not valid UTF-8")
            })?,
        )?;
        parse_state(
            std::str::from_utf8(&state)
                .map_err(|_| JobError::new("SEL-J007", "pending job state is not valid UTF-8"))?,
        )?;
        (Some(events), Some(state))
    } else {
        (None, None)
    };

    if !current_is_committed {
        let events = candidate_events.as_deref().ok_or_else(|| {
            JobError::new("SEL-J007", "job commit transaction candidates are missing")
        })?;
        let state = candidate_state.as_deref().ok_or_else(|| {
            JobError::new("SEL-J007", "job commit transaction candidates are missing")
        })?;
        if current_events_sha256 != pending.events_sha256 {
            atomic_write(&event_path, events)?;
        }
        if current_state_sha256 != pending.state_sha256 {
            atomic_write(&state_path, state)?;
        }
    }

    let final_events = fs::read(&event_path)
        .map_err(|error| JobError::io("SEL-J007", "failed to verify the job event log", error))?;
    let final_state = fs::read(&state_path)
        .map_err(|error| JobError::io("SEL-J007", "failed to verify job state", error))?;
    if crate::sha256_hex(&final_events) != pending.events_sha256
        || crate::sha256_hex(&final_state) != pending.state_sha256
    {
        return Err(JobError::new(
            "SEL-J010",
            format!(
                "job commit sequence {} could not be verified after replacement",
                pending.sequence
            ),
        ));
    }
    if temporary_exists {
        remove_commit_directory(&temporary).map_err(|error| {
            JobError::io(
                "SEL-J010",
                "failed to clean the completed job commit transaction",
                error,
            )
        })?;
    }
    fs::remove_file(&marker_path).map_err(|error| {
        JobError::io(
            "SEL-J010",
            "failed to clear the completed job commit transaction",
            error,
        )
    })?;
    sync_directory(directory);
    Ok(())
}

fn remove_commit_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "job commit transaction directory is not a regular directory",
        ));
    }
    for name in ["events.jsonl", "state.json"] {
        let child = path.join(name);
        match fs::symlink_metadata(&child) {
            Ok(metadata) => {
                if metadata_is_reparse(&metadata) || !metadata.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "job commit transaction contains an unexpected file",
                    ));
                }
                fs::remove_file(child)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    fs::remove_dir(path)
}

pub(crate) fn render_human(value: &JsonValue) -> String {
    let operation = object_string(value, "operation").unwrap_or("job");
    if operation == "list" {
        let count = match value {
            JsonValue::Object(object) => match object.get("jobs") {
                Some(JsonValue::Array(jobs)) => jobs.len(),
                _ => 0,
            },
            _ => 0,
        };
        return format!("jobs={count}\n");
    }
    let job_id = object_string(value, "job_id").unwrap_or("unknown");
    let status = object_string(value, "status").unwrap_or("unknown");
    let reused = match value {
        JsonValue::Object(object) => matches!(object.get("reused"), Some(JsonValue::Bool(true))),
        _ => false,
    };
    if operation == "create" && reused {
        format!("job {job_id} status={status} reused=true\n")
    } else {
        format!("job {job_id} status={status}\n")
    }
}

fn validate_job_id(value: &str) -> Result<(), JobError> {
    if value.is_empty() || value.len() > MAX_JOB_ID_BYTES {
        return Err(JobError::new(
            "SEL-J003",
            format!("job id must contain 1 to {MAX_JOB_ID_BYTES} ASCII bytes"),
        ));
    }
    if !value.bytes().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphanumeric() || byte == b'_' || (byte == b'-' && index > 0)
    }) || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(JobError::new(
            "SEL-J003",
            "job id must start with a letter or digit and contain only letters, digits, _ or -",
        ));
    }
    Ok(())
}

fn validate_identity(value: &str, label: &str) -> Result<(), JobError> {
    if value.is_empty() || value.len() > MAX_JOB_ID_BYTES || !value.is_ascii() {
        return Err(JobError::new(
            "SEL-J014",
            format!("{label} must contain 1 to {MAX_JOB_ID_BYTES} ASCII bytes"),
        ));
    }
    if !value.bytes().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphanumeric() || byte == b'_' || (byte == b'-' && index > 0)
    }) || !value.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(JobError::new(
            "SEL-J014",
            format!(
                "{label} must start with a letter or digit and contain only letters, digits, _ or -"
            ),
        ));
    }
    Ok(())
}

fn validate_lease_id(value: &str) -> Result<(), JobError> {
    validate_identity(value, "lease id")
}

fn validate_ttl(ttl_seconds: u64) -> Result<(), JobError> {
    if !(1..=3600).contains(&ttl_seconds) {
        return Err(JobError::new(
            "SEL-J020",
            "lease TTL must be between 1 and 3600 seconds",
        ));
    }
    Ok(())
}

fn lease_active(state: &JobState, now: u64) -> bool {
    state
        .lease_expires_at
        .is_some_and(|expires_at| expires_at > now)
}

fn require_lease(state: &JobState, lease_id: &str) -> Result<(), JobError> {
    if state.lease_id.as_deref() != Some(lease_id) {
        return Err(JobError::new("SEL-J017", "lease id is invalid"));
    }
    Ok(())
}

fn validate_transition(from: Status, to: Status) -> Result<(), JobError> {
    let allowed = match from {
        Status::Accepted => matches!(
            to,
            Status::Queued | Status::Starting | Status::Cancelling | Status::Failed
        ),
        Status::Queued => matches!(to, Status::Starting | Status::Cancelling | Status::Failed),
        Status::Starting => {
            matches!(
                to,
                Status::Running
                    | Status::Queued
                    | Status::Cancelling
                    | Status::Failed
                    | Status::Lost
            )
        }
        Status::Running => {
            matches!(
                to,
                Status::Committing | Status::Cancelling | Status::Failed | Status::Lost
            )
        }
        Status::Committing => matches!(to, Status::Succeeded | Status::Failed | Status::Lost),
        Status::Cancelling => matches!(to, Status::Cancelled | Status::Failed),
        Status::Lost => matches!(to, Status::RetryWait | Status::Failed | Status::Queued),
        Status::RetryWait => matches!(to, Status::Queued | Status::Failed),
        Status::Succeeded => false,
        Status::Cancelled | Status::Failed => matches!(to, Status::Queued),
    };
    if allowed {
        Ok(())
    } else {
        Err(JobError::new(
            "SEL-J005",
            format!(
                "invalid job transition {} -> {}",
                from.as_str(),
                to.as_str()
            ),
        ))
    }
}

fn is_executor_owned(status: Status) -> bool {
    matches!(
        status,
        Status::Starting
            | Status::Running
            | Status::Committing
            | Status::Succeeded
            | Status::Cancelling
            | Status::Cancelled
            | Status::Failed
            | Status::Lost
            | Status::RetryWait
    )
}

fn validate_event(state: &JobState, event: &Event) -> Result<(), JobError> {
    if event.from != Some(state.status) {
        return Err(JobError::new(
            "SEL-J007",
            "job event predecessor does not match the derived state",
        ));
    }
    match event.kind.as_str() {
        "admin" => {
            if state.status != Status::Accepted
                || event.to != Status::Queued
                || event.attempt != Some(state.attempt)
                || event.owner != state.owner
                || event.attempt_id != state.attempt_id
                || event.claimed_at != state.claimed_at
                || event.lease_id != state.lease_id
                || event.lease_expires_at != state.lease_expires_at
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "admin event is not the accepted-to-queued transition",
                ));
            }
        }
        "claim" => {
            let attempt = event
                .attempt
                .ok_or_else(|| JobError::new("SEL-J007", "claim event has no attempt"))?;
            let next_attempt = state.attempt.checked_add(1);
            if !matches!(state.status, Status::Accepted | Status::Queued)
                || event.to != Status::Starting
                || (attempt != state.attempt && Some(attempt) != next_attempt)
                || event.owner.is_none()
                || event.attempt_id.is_none()
                || event.claimed_at != Some(event.at)
                || event.lease_id.is_none()
                || event.lease_expires_at.is_none()
                || event.lease_expires_at <= Some(event.at)
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "claim event has invalid lease data",
                ));
            }
            validate_identity(event.owner.as_deref().unwrap_or_default(), "lease owner")?;
            validate_identity(
                event.attempt_id.as_deref().unwrap_or_default(),
                "attempt id",
            )?;
            validate_lease_id(event.lease_id.as_deref().unwrap_or_default())?;
            validate_transition(state.status, event.to)?;
        }
        "heartbeat" => {
            if !matches!(
                state.status,
                Status::Starting | Status::Running | Status::Committing | Status::Cancelling
            ) || event.to != state.status
                || event.attempt != Some(state.attempt)
                || event.owner != state.owner
                || event.attempt_id != state.attempt_id
                || event.claimed_at != state.claimed_at
                || event.lease_id != state.lease_id
                || event.lease_expires_at <= Some(event.at)
                || event.lease_expires_at <= state.lease_expires_at
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "heartbeat event does not extend the active lease",
                ));
            }
        }
        "executor" => {
            let terminal = matches!(
                event.to,
                Status::Succeeded | Status::Cancelled | Status::Failed
            );
            if !matches!(
                state.status,
                Status::Starting | Status::Running | Status::Committing | Status::Cancelling
            ) || event.attempt != Some(state.attempt)
                || event.lease_id
                    != if terminal {
                        None
                    } else {
                        state.lease_id.clone()
                    }
                || event.lease_expires_at
                    != if terminal {
                        None
                    } else {
                        state.lease_expires_at
                    }
                || event.owner != if terminal { None } else { state.owner.clone() }
                || event.attempt_id
                    != if terminal {
                        None
                    } else {
                        state.attempt_id.clone()
                    }
                || event.claimed_at != if terminal { None } else { state.claimed_at }
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "executor event changed lease identity",
                ));
            }
            validate_transition(state.status, event.to)?;
        }
        "retry" => {
            if !matches!(
                state.status,
                Status::Lost | Status::Cancelled | Status::Failed
            ) || event.to != Status::Queued
                || event.attempt != Some(state.attempt)
                || event.owner.is_some()
                || event.attempt_id.is_some()
                || event.claimed_at.is_some()
                || event.lease_id.is_some()
                || event.lease_expires_at.is_some()
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "retry event did not clear the terminal attempt identity",
                ));
            }
            validate_transition(state.status, event.to)?;
        }
        "release" => {
            if state.status != Status::Starting
                || event.to != Status::Queued
                || event.attempt != Some(state.attempt)
                || event.owner.is_some()
                || event.attempt_id.is_some()
                || event.claimed_at.is_some()
                || event.lease_id.is_some()
                || event.lease_expires_at.is_some()
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "release event did not clear the lease",
                ));
            }
            validate_transition(state.status, event.to)?;
        }
        "recover" => {
            if !matches!(
                state.status,
                Status::Starting | Status::Running | Status::Committing | Status::Cancelling
            ) || event.to != Status::Lost
                || event.attempt != Some(state.attempt)
                || event.owner != state.owner
                || event.attempt_id != state.attempt_id
                || event.claimed_at != state.claimed_at
                || event.lease_id.is_some()
                || event.lease_expires_at.is_some()
                || state
                    .lease_expires_at
                    .is_none_or(|expires| expires > event.at)
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "recover event has invalid expiry data",
                ));
            }
            validate_transition(state.status, event.to)?;
        }
        "legacy" | "transition" => {
            validate_transition(state.status, event.to)?;
            if event.attempt.is_some() && event.attempt != Some(state.attempt) {
                return Err(JobError::new(
                    "SEL-J007",
                    "legacy event changed the attempt",
                ));
            }
            if event.owner != state.owner
                || event.attempt_id != state.attempt_id
                || event.claimed_at != state.claimed_at
                || event.lease_id != state.lease_id
                || event.lease_expires_at != state.lease_expires_at
            {
                return Err(JobError::new(
                    "SEL-J007",
                    "legacy event changed lease identity",
                ));
            }
        }
        _ => return Err(JobError::new("SEL-J007", "job event kind is unsupported")),
    }
    Ok(())
}

struct Record {
    state: JobState,
}

struct JobLock {
    _file: File,
}

impl JobLock {
    fn acquire(directory: &Path) -> Result<Self, JobError> {
        let path = directory.join("job.lock");
        reject_symlink(&path)?;
        let file = open_lock_file(&path).map_err(|error| {
            if path_is_reparse(&path) {
                JobError::new("SEL-J012", "job store contains a symlink or reparse point")
            } else {
                JobError::io("SEL-J010", "failed to open the job lock", error)
            }
        })?;
        let metadata = file
            .metadata()
            .map_err(|error| JobError::io("SEL-J010", "failed to inspect the job lock", error))?;
        if metadata_is_reparse(&metadata) {
            return Err(JobError::new(
                "SEL-J012",
                "job store contains a symlink or reparse point",
            ));
        }
        if !metadata.is_file() {
            return Err(JobError::new("SEL-J007", "job lock is not a regular file"));
        }
        reject_symlink(&path)?;
        for _ in 0..LOCK_WAIT_ATTEMPTS {
            match try_lock(&file) {
                Ok(true) => return Ok(Self { _file: file }),
                Ok(false) => thread::sleep(LOCK_WAIT_STEP),
                Err(error) => {
                    return Err(JobError::io("SEL-J010", "failed to lock the job", error));
                }
            }
        }
        Err(JobError::new(
            "SEL-J011",
            "job is busy; no transition was committed",
        ))
    }
}

fn open_lock_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    configure_lock_open(&mut options);
    options.open(path)
}

#[cfg(unix)]
fn configure_lock_open(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;

    // Reject a replaced final path component instead of following it. These
    // are the values exposed by the supported POSIX kernels in CI; other Unix
    // targets retain the metadata checks below but may not have this flag.
    #[cfg(target_os = "linux")]
    const NO_FOLLOW: i32 = 0x20000;
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    const NO_FOLLOW: i32 = 0x100;
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "ios")))]
    const NO_FOLLOW: i32 = 0;

    options.custom_flags(NO_FOLLOW);
}

#[cfg(windows)]
fn configure_lock_open(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;

    // Open the final reparse point itself so its attributes can be rejected
    // instead of following a junction/symlink to an attacker-selected file.
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(unix, windows)))]
fn configure_lock_open(_options: &mut OpenOptions) {}

fn path_is_reparse(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata_is_reparse(&metadata))
        .unwrap_or(false)
}

#[cfg(unix)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;

    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    const EAGAIN: i32 = 11;
    const EWOULDBLOCK: i32 = 35;

    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    let result = unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) };
    if result == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if matches!(error.raw_os_error(), Some(code) if code == EAGAIN || code == EWOULDBLOCK) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(windows)]
fn try_lock(file: &File) -> io::Result<bool> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let mut overlapped = OVERLAPPED::default();
    let result = unsafe {
        LockFileEx(
            file.as_raw_handle() as _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if result != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock || error.raw_os_error() == Some(33) {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(not(any(unix, windows)))]
fn try_lock(_file: &File) -> io::Result<bool> {
    Ok(true)
}

fn load_record(directory: &Path) -> Result<Record, JobError> {
    let _lock = JobLock::acquire(directory)?;
    load_record_unlocked(directory)
}

fn load_record_unlocked(directory: &Path) -> Result<Record, JobError> {
    let record = load_record_unlocked_read_only(directory)?;
    let state_path = directory.join("state.json");
    let state_source = read_required(&state_path, "job state")?;
    let stored = parse_state(&state_source)?;
    if record.state != stored {
        atomic_write(&state_path, state_json(&record.state).as_bytes())?;
    }
    Ok(record)
}

fn load_record_unlocked_read_only(directory: &Path) -> Result<Record, JobError> {
    load_record_unlocked_read_only_with_spec(directory, true)
}

fn load_record_unlocked_read_only_without_spec(directory: &Path) -> Result<Record, JobError> {
    load_record_unlocked_read_only_with_spec(directory, false)
}

fn load_record_unlocked_read_only_with_spec(
    directory: &Path,
    verify_spec_hash: bool,
) -> Result<Record, JobError> {
    reject_symlink(directory)?;
    recover_pending_commit(directory)?;
    let state_path = directory.join("state.json");
    reject_symlink(&state_path)?;
    let state_source = read_required(&state_path, "job state")?;
    let state = parse_state(&state_source)?;
    validate_job_id(&state.job_id)?;
    if directory.file_name().and_then(|value| value.to_str()) != Some(state.job_id.as_str()) {
        return Err(JobError::new(
            "SEL-J007",
            "job directory name does not match state.json job_id",
        ));
    }
    if verify_spec_hash {
        let spec_path = directory.join("spec.json");
        reject_symlink(&spec_path)?;
        let spec_source = read_required(&spec_path, "job spec")?;
        let spec_value =
            json::parse(&spec_source, MAX_SPEC_BYTES, MAX_JSON_DEPTH).map_err(|error| {
                JobError::new(
                    "SEL-J007",
                    format!("job spec is invalid: {}", error.message),
                )
            })?;
        let expected_spec_sha256 =
            crate::sha256_hex(format!("{}\n", json::encode(&spec_value)).as_bytes());
        if expected_spec_sha256 != state.spec_sha256 {
            return Err(JobError::new(
                "SEL-J007",
                "job spec hash does not match state.json",
            ));
        }
    }
    let events_path = directory.join("events.jsonl");
    reject_symlink(&events_path)?;
    let events_source = read_required(&events_path, "job event log")?;
    let events = parse_events(&events_source)?;
    if events.is_empty() {
        return Err(JobError::new("SEL-J007", "job event log is empty"));
    }
    let derived = derive_state(&events, &state)?;
    Ok(Record { state: derived })
}

fn derive_state(events: &[Event], stored: &JobState) -> Result<JobState, JobError> {
    let first = events.first().expect("checked by caller");
    if first.sequence != 0
        || first.from.is_some()
        || first.to != Status::Accepted
        || first.job_id != stored.job_id
        || first.spec_sha256 != stored.spec_sha256
    {
        return Err(JobError::new(
            "SEL-J007",
            "job event log has an invalid creation event",
        ));
    }
    let creation_attempt_is_valid = match first.kind.as_str() {
        "create" => first.attempt == Some(1),
        "legacy" | "transition" => first.attempt.is_none() || first.attempt == Some(1),
        _ => false,
    };
    if first.owner.is_some()
        || first.attempt_id.is_some()
        || first.claimed_at.is_some()
        || first.lease_id.is_some()
        || first.lease_expires_at.is_some()
        || !creation_attempt_is_valid
    {
        return Err(JobError::new(
            "SEL-J007",
            "job event log has invalid creation identity",
        ));
    }
    let mut state = JobState {
        job_id: first.job_id.clone(),
        status: first.to,
        sequence: 0,
        attempt: first.attempt.unwrap_or(1),
        spec_sha256: first.spec_sha256.clone(),
        created_at: first.at,
        updated_at: first.at,
        owner: first.owner.clone(),
        attempt_id: first.attempt_id.clone(),
        claimed_at: first.claimed_at,
        lease_id: first.lease_id.clone(),
        lease_expires_at: first.lease_expires_at,
    };
    for (expected, event) in events.iter().enumerate().skip(1) {
        let expected = expected as u64;
        if event.sequence != expected
            || event.job_id != state.job_id
            || event.spec_sha256 != state.spec_sha256
            || event.from != Some(state.status)
        {
            return Err(JobError::new(
                "SEL-J007",
                "job event log contains a broken sequence or predecessor",
            ));
        }
        validate_event(&state, event)?;
        state.status = event.to;
        state.sequence = event.sequence;
        state.updated_at = event.at;
        state.attempt = event.attempt.unwrap_or(state.attempt);
        state.owner = event.owner.clone();
        state.attempt_id = event.attempt_id.clone();
        state.claimed_at = event.claimed_at;
        state.lease_id = event.lease_id.clone();
        state.lease_expires_at = event.lease_expires_at;
    }
    if stored.job_id != state.job_id
        || stored.attempt != state.attempt
        || stored.created_at != state.created_at
        || stored.spec_sha256 != state.spec_sha256
        || stored.sequence > state.sequence
        || (stored.sequence == state.sequence
            && (stored.status != state.status
                || stored.owner != state.owner
                || stored.attempt_id != state.attempt_id
                || stored.claimed_at != state.claimed_at
                || stored.lease_id != state.lease_id
                || stored.lease_expires_at != state.lease_expires_at))
    {
        return Err(JobError::new(
            "SEL-J007",
            "state.json does not match the authoritative job event log",
        ));
    }
    let stored_event = events
        .get(stored.sequence as usize)
        .ok_or_else(|| JobError::new("SEL-J007", "state sequence is outside the event log"))?;
    if stored.status != stored_event.to || stored.updated_at != stored_event.at {
        return Err(JobError::new(
            "SEL-J007",
            "state snapshot does not match its event sequence",
        ));
    }
    validate_state_identity(&state)?;
    Ok(state)
}

fn validate_state_identity(state: &JobState) -> Result<(), JobError> {
    if state.attempt == 0 {
        return Err(JobError::new(
            "SEL-J007",
            "job attempt must be a positive integer",
        ));
    }
    if let Some(owner) = &state.owner {
        validate_identity(owner, "lease owner")?;
    }
    if let Some(attempt_id) = &state.attempt_id {
        validate_identity(attempt_id, "attempt id")?;
    }
    if let Some(lease_id) = &state.lease_id {
        validate_lease_id(lease_id)?;
    }
    if state.lease_id.is_some() != state.lease_expires_at.is_some() {
        return Err(JobError::new(
            "SEL-J007",
            "job lease identity has an incomplete expiry pair",
        ));
    }
    if state.lease_id.is_some()
        && (state.owner.is_none() || state.attempt_id.is_none() || state.claimed_at.is_none())
    {
        return Err(JobError::new(
            "SEL-J007",
            "job lease identity is incomplete",
        ));
    }
    Ok(())
}

fn parse_state(source: &str) -> Result<JobState, JobError> {
    let value = json::parse(source, 16 * 1024, 8).map_err(|error| {
        JobError::new(
            "SEL-J007",
            format!("job state is invalid: {}", error.message),
        )
    })?;
    let object = object(value, "job state")?;
    if object_string_from(&object, "schema")? != JOB_SCHEMA {
        return Err(JobError::new("SEL-J007", "job state schema is unsupported"));
    }
    Ok(JobState {
        job_id: object_string_from(&object, "job_id")?.to_owned(),
        status: Status::parse(object_string_from(&object, "status")?)?,
        sequence: object_u64(&object, "sequence")?,
        attempt: object_u64(&object, "attempt")?,
        spec_sha256: object_string_from(&object, "spec_sha256")?.to_owned(),
        created_at: object_u64(&object, "created_at")?,
        updated_at: object_u64(&object, "updated_at")?,
        owner: object_optional_string(&object, "owner")?.map(ToOwned::to_owned),
        attempt_id: object_optional_string(&object, "attempt_id")?.map(ToOwned::to_owned),
        claimed_at: object_optional_u64(&object, "claimed_at")?,
        lease_id: object_optional_string(&object, "lease_id")?.map(ToOwned::to_owned),
        lease_expires_at: object_optional_u64(&object, "lease_expires_at")?,
    })
}

fn parse_events(source: &str) -> Result<Vec<Event>, JobError> {
    let mut events = Vec::new();
    for line in source.lines() {
        let value = json::parse(line, 64 * 1024, 12).map_err(|error| {
            JobError::new(
                "SEL-J007",
                format!("job event log contains invalid JSON: {}", error.message),
            )
        })?;
        let object = object(value, "job event")?;
        if object_string_from(&object, "schema")? != EVENT_SCHEMA {
            return Err(JobError::new("SEL-J007", "job event schema is unsupported"));
        }
        let from = match object.get("from") {
            Some(JsonValue::Null) => None,
            Some(JsonValue::String(value)) => Some(Status::parse(value)?),
            _ => {
                return Err(JobError::new(
                    "SEL-J007",
                    "job event from must be null or a status",
                ));
            }
        };
        events.push(Event {
            sequence: object_u64(&object, "sequence")?,
            job_id: object_string_from(&object, "job_id")?.to_owned(),
            from,
            to: Status::parse(object_string_from(&object, "to")?)?,
            at: object_u64(&object, "at")?,
            spec_sha256: object_string_from(&object, "spec_sha256")?.to_owned(),
            attempt: object_optional_u64(&object, "attempt")?,
            kind: object_optional_string(&object, "kind")?
                .unwrap_or("legacy")
                .to_owned(),
            owner: object_optional_string(&object, "owner")?.map(ToOwned::to_owned),
            attempt_id: object_optional_string(&object, "attempt_id")?.map(ToOwned::to_owned),
            claimed_at: object_optional_u64(&object, "claimed_at")?,
            lease_id: object_optional_string(&object, "lease_id")?.map(ToOwned::to_owned),
            lease_expires_at: object_optional_u64(&object, "lease_expires_at")?,
        });
    }
    if source.is_empty() || !source.ends_with('\n') {
        return Err(JobError::new(
            "SEL-J008",
            "job event log does not end at a complete line",
        ));
    }
    Ok(events)
}

fn object(value: JsonValue, name: &str) -> Result<BTreeMap<String, JsonValue>, JobError> {
    match value {
        JsonValue::Object(object) => Ok(object),
        _ => Err(JobError::new(
            "SEL-J007",
            format!("{name} must be a JSON object"),
        )),
    }
}

fn object_string_from<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<&'a str, JobError> {
    match object.get(key) {
        Some(JsonValue::String(value)) => Ok(value),
        Some(_) => Err(JobError::new(
            "SEL-J007",
            format!("job field {key} must be a string"),
        )),
        None => Err(JobError::new(
            "SEL-J007",
            format!("job field {key} is missing"),
        )),
    }
}

fn object_u64(object: &BTreeMap<String, JsonValue>, key: &str) -> Result<u64, JobError> {
    match object.get(key) {
        Some(JsonValue::Number(value)) => value.parse::<u64>().map_err(|_| {
            JobError::new(
                "SEL-J007",
                format!("job field {key} must be a non-negative integer"),
            )
        }),
        Some(_) => Err(JobError::new(
            "SEL-J007",
            format!("job field {key} must be an integer"),
        )),
        None => Err(JobError::new(
            "SEL-J007",
            format!("job field {key} is missing"),
        )),
    }
}

fn object_string<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
    match value {
        JsonValue::Object(object) => match object.get(key) {
            Some(JsonValue::String(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

fn object_optional_string<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<Option<&'a str>, JobError> {
    match object.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::String(value)) => Ok(Some(value)),
        Some(_) => Err(JobError::new(
            "SEL-J007",
            format!("job field {key} must be a string or null"),
        )),
    }
}

fn object_optional_u64(
    object: &BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<Option<u64>, JobError> {
    match object.get(key) {
        None | Some(JsonValue::Null) => Ok(None),
        Some(JsonValue::Number(value)) => value.parse::<u64>().map(Some).map_err(|_| {
            JobError::new(
                "SEL-J007",
                format!("job field {key} must be a non-negative integer or null"),
            )
        }),
        Some(_) => Err(JobError::new(
            "SEL-J007",
            format!("job field {key} must be an integer or null"),
        )),
    }
}

fn state_json(state: &JobState) -> String {
    json::encode(&JsonValue::Object(BTreeMap::from([
        (
            "attempt_id".to_owned(),
            state
                .attempt_id
                .clone()
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "attempt".to_owned(),
            JsonValue::Number(state.attempt.to_string()),
        ),
        (
            "created_at".to_owned(),
            JsonValue::Number(state.created_at.to_string()),
        ),
        (
            "claimed_at".to_owned(),
            state
                .claimed_at
                .map(|value| JsonValue::Number(value.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        ("job_id".to_owned(), JsonValue::String(state.job_id.clone())),
        (
            "lease_expires_at".to_owned(),
            state
                .lease_expires_at
                .map(|value| JsonValue::Number(value.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "lease_id".to_owned(),
            state
                .lease_id
                .clone()
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "owner".to_owned(),
            state
                .owner
                .clone()
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "schema".to_owned(),
            JsonValue::String(JOB_SCHEMA.to_owned()),
        ),
        (
            "sequence".to_owned(),
            JsonValue::Number(state.sequence.to_string()),
        ),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(state.spec_sha256.clone()),
        ),
        (
            "status".to_owned(),
            JsonValue::String(state.status.as_str().to_owned()),
        ),
        (
            "updated_at".to_owned(),
            JsonValue::Number(state.updated_at.to_string()),
        ),
    ]))) + "\n"
}

fn event_line(event: &Event) -> String {
    json::encode(&JsonValue::Object(BTreeMap::from([
        (
            "attempt".to_owned(),
            event
                .attempt
                .map(|value| JsonValue::Number(value.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "attempt_id".to_owned(),
            event
                .attempt_id
                .clone()
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null),
        ),
        ("at".to_owned(), JsonValue::Number(event.at.to_string())),
        (
            "claimed_at".to_owned(),
            event
                .claimed_at
                .map(|value| JsonValue::Number(value.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "from".to_owned(),
            event
                .from
                .map(|status| JsonValue::String(status.as_str().to_owned()))
                .unwrap_or(JsonValue::Null),
        ),
        ("job_id".to_owned(), JsonValue::String(event.job_id.clone())),
        ("kind".to_owned(), JsonValue::String(event.kind.clone())),
        (
            "lease_expires_at".to_owned(),
            event
                .lease_expires_at
                .map(|value| JsonValue::Number(value.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "lease_id".to_owned(),
            event
                .lease_id
                .clone()
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "owner".to_owned(),
            event
                .owner
                .clone()
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null),
        ),
        (
            "schema".to_owned(),
            JsonValue::String(EVENT_SCHEMA.to_owned()),
        ),
        (
            "sequence".to_owned(),
            JsonValue::Number(event.sequence.to_string()),
        ),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(event.spec_sha256.clone()),
        ),
        (
            "to".to_owned(),
            JsonValue::String(event.to.as_str().to_owned()),
        ),
    ]))) + "\n"
}

fn job_summary(record: &Record) -> JsonValue {
    let mut summary = BTreeMap::from([
        (
            "attempt".to_owned(),
            JsonValue::Number(record.state.attempt.to_string()),
        ),
        (
            "job_id".to_owned(),
            JsonValue::String(record.state.job_id.clone()),
        ),
        (
            "sequence".to_owned(),
            JsonValue::Number(record.state.sequence.to_string()),
        ),
        (
            "spec_sha256".to_owned(),
            JsonValue::String(record.state.spec_sha256.clone()),
        ),
        (
            "status".to_owned(),
            JsonValue::String(record.state.status.as_str().to_owned()),
        ),
        (
            "updated_at".to_owned(),
            JsonValue::Number(record.state.updated_at.to_string()),
        ),
    ]);
    if let Some(owner) = &record.state.owner {
        summary.insert("owner".to_owned(), JsonValue::String(owner.clone()));
    }
    if let Some(attempt_id) = &record.state.attempt_id {
        summary.insert(
            "attempt_id".to_owned(),
            JsonValue::String(attempt_id.clone()),
        );
    }
    if let Some(claimed_at) = record.state.claimed_at {
        summary.insert(
            "claimed_at".to_owned(),
            JsonValue::Number(claimed_at.to_string()),
        );
    }
    if let Some(lease_id) = &record.state.lease_id {
        summary.insert("lease_id".to_owned(), JsonValue::String(lease_id.clone()));
    }
    if let Some(lease_expires_at) = record.state.lease_expires_at {
        summary.insert(
            "lease_expires_at".to_owned(),
            JsonValue::Number(lease_expires_at.to_string()),
        );
    }
    JsonValue::Object(summary)
}

fn archive_result(directory: &Path, attempt: u64) -> Result<(), JobError> {
    let source = directory.join("result.json");
    reject_symlink(&source)?;
    let bytes = match fs::read(&source) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to read the previous job result",
                error,
            ));
        }
    };
    let attempts = directory.join("attempts");
    ensure_directory(&attempts, "job attempt archive")?;
    let destination = attempts.join(format!("attempt-{attempt}.json"));
    atomic_write(&destination, &bytes)
}

fn result_json(
    operation: Operation,
    record: &Record,
    reused: Option<bool>,
    transition: Option<Status>,
) -> JsonValue {
    let mut result = match job_summary(record) {
        JsonValue::Object(value) => value,
        _ => unreachable!(),
    };
    result.insert(
        "operation".to_owned(),
        JsonValue::String(operation.as_str().to_owned()),
    );
    result.insert(
        "schema".to_owned(),
        JsonValue::String(JOB_SCHEMA.to_owned()),
    );
    if let Some(reused) = reused {
        result.insert("reused".to_owned(), JsonValue::Bool(reused));
    }
    if let Some(status) = transition {
        result.insert(
            "transition".to_owned(),
            JsonValue::String(status.as_str().to_owned()),
        );
    }
    JsonValue::Object(result)
}

fn read_required(path: &Path, name: &str) -> Result<String, JobError> {
    fs::read_to_string(path)
        .map_err(|error| JobError::io("SEL-J007", &format!("failed to read {name}"), error))
}

fn jobs_directory(root: &Path) -> PathBuf {
    root.join(".seloryn").join("jobs")
}

fn job_directory(root: &Path, job_id: &str) -> PathBuf {
    jobs_directory(root).join(job_id)
}

fn ensure_root_directory(root: &Path) -> Result<(), JobError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| JobError::io("SEL-J007", "failed to inspect the job root", error))?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(JobError::new("SEL-J007", "job root is not a directory"));
    }
    Ok(())
}

fn ensure_directory(path: &Path, name: &str) -> Result<(), JobError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if is_existing_directory_error(&error) => {
            reject_symlink(path)?;
            let metadata = fs::symlink_metadata(path).map_err(|inspect_error| {
                JobError::io("SEL-J010", "failed to inspect job storage", inspect_error)
            })?;
            if metadata.is_dir() {
                Ok(())
            } else {
                Err(JobError::new(
                    "SEL-J007",
                    format!("{name} is not a directory"),
                ))
            }
        }
        Err(error) => Err(JobError::io(
            "SEL-J010",
            &format!("failed to create {name}"),
            error,
        )),
    }
}

fn ensure_store_contained(root: &Path, jobs_root: &Path) -> Result<(), JobError> {
    reject_symlink(root)?;
    reject_symlink(&root.join(".seloryn"))?;
    reject_symlink(jobs_root)?;
    let canonical_root = fs::canonicalize(root)
        .map_err(|error| JobError::io("SEL-J007", "failed to resolve the job root", error))?;
    let canonical_jobs = fs::canonicalize(jobs_root)
        .map_err(|error| JobError::io("SEL-J007", "failed to resolve the job store", error))?;
    if !canonical_jobs.starts_with(&canonical_root) {
        return Err(JobError::new(
            "SEL-J012",
            "job store resolves outside the selected root",
        ));
    }
    Ok(())
}

fn ensure_job_contained(root: &Path, directory: &Path) -> Result<(), JobError> {
    let jobs_root = directory
        .parent()
        .ok_or_else(|| JobError::new("SEL-J012", "job directory has no store parent"))?;
    ensure_store_contained(root, jobs_root)?;
    reject_symlink(directory)?;
    let canonical_jobs = fs::canonicalize(jobs_root)
        .map_err(|error| JobError::io("SEL-J007", "failed to resolve the job store", error))?;
    let canonical_directory = fs::canonicalize(directory)
        .map_err(|error| JobError::io("SEL-J007", "failed to resolve the job directory", error))?;
    if !canonical_directory.starts_with(&canonical_jobs) {
        return Err(JobError::new(
            "SEL-J012",
            "job directory resolves outside the selected store",
        ));
    }
    Ok(())
}

fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn reject_symlink(path: &Path) -> Result<(), JobError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_reparse(&metadata) => Err(JobError::new(
            "SEL-J012",
            "job store contains a symlink or reparse point",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(JobError::io(
            "SEL-J010",
            "failed to inspect job storage",
            error,
        )),
    }
}

fn is_existing_directory_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::AlreadyExists
        // POSIX reports a non-empty directory target as ENOTEMPTY when the
        // atomic publish races with or reuses an existing job directory.
        || matches!(error.raw_os_error(), Some(39) | Some(66) | Some(145) | Some(183))
}

fn generated_job_id() -> String {
    let entropy = format!(
        "{}:{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default(),
        crate::PACKAGE_VERSION,
    );
    format!("job-{}", &crate::sha256_hex(entropy.as_bytes())[..24])
}

fn generated_owner() -> String {
    format!("cli-{}", std::process::id())
}

fn generated_attempt_id(owner: &str) -> String {
    let entropy = format!(
        "{}:{}:{}:{}",
        owner,
        std::process::id(),
        unix_nanos(),
        crate::PACKAGE_VERSION,
    );
    format!("attempt-{}", &crate::sha256_hex(entropy.as_bytes())[..24])
}

fn generated_lease_id(owner: &str, attempt_id: &str) -> String {
    let entropy = format!(
        "{}:{}:{}:{}",
        owner,
        attempt_id,
        unix_nanos(),
        crate::PACKAGE_VERSION,
    );
    format!("lease-{}", &crate::sha256_hex(entropy.as_bytes())[..32])
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn lease_deadline(ttl_seconds: u64) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| lease_deadline_from_duration(duration, ttl_seconds))
        .unwrap_or(ttl_seconds)
}

fn lease_deadline_from_duration(duration: Duration, ttl_seconds: u64) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() != 0))
        .saturating_add(ttl_seconds)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), JobError> {
    let parent = path
        .parent()
        .ok_or_else(|| JobError::new("SEL-J010", "cannot determine job storage parent"))?;
    reject_symlink(parent)?;
    reject_symlink(path)?;
    fs::create_dir_all(parent)
        .map_err(|error| JobError::io("SEL-J010", "failed to create job storage", error))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| JobError::new("SEL-J010", "job storage path is not valid UTF-8"))?;
    let temporary = parent.join(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unix_nanos()
    ));
    let write_result = (|| -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)?;
        sync_directory(parent);
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
        .map_err(|error| JobError::io("SEL-J010", "failed to atomically write job storage", error))
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(temporary, destination)
}

#[cfg(windows)]
fn replace_file(temporary: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::time::Duration;

    #[test]
    fn lease_deadline_rounds_fractional_second_up_before_adding_ttl() {
        assert_eq!(lease_deadline_from_duration(Duration::new(12, 0), 1), 13);
        assert_eq!(lease_deadline_from_duration(Duration::new(12, 1), 1), 14);
    }

    #[test]
    fn pending_commit_recovery_reconciles_event_first_replacement() {
        let root = std::env::temp_dir().join(format!(
            "seloryn-job-commit-recovery-{}-{}",
            std::process::id(),
            unix_nanos()
        ));
        fs::create_dir_all(&root).expect("test root");
        let spec_path = root.join("spec.json");
        fs::write(&spec_path, b"{\"kind\":\"test\"}\n").expect("test spec");
        create(&root, &spec_path, Some(OsStr::new("job"))).expect("create job");

        let directory = job_directory(&root, "job");
        let event_path = directory.join("events.jsonl");
        let state_path = directory.join("state.json");
        let base_events = fs::read(&event_path).expect("base events");
        let base_state = fs::read(&state_path).expect("base state");
        let record = load_record(&directory).expect("load base record");
        let event = Event {
            sequence: 1,
            job_id: "job".to_owned(),
            from: Some(Status::Accepted),
            to: Status::Queued,
            at: record.state.updated_at.saturating_add(1),
            spec_sha256: record.state.spec_sha256.clone(),
            attempt: Some(record.state.attempt),
            kind: "admin".to_owned(),
            owner: None,
            attempt_id: None,
            claimed_at: None,
            lease_id: None,
            lease_expires_at: None,
        };
        let mut next_state = record.state.clone();
        apply_event(&mut next_state, &event);
        let mut next_events = base_events.clone();
        next_events.extend_from_slice(event_line(&event).as_bytes());
        let pending = PendingCommit {
            temp_directory: format!("{COMMIT_DIRECTORY_PREFIX}test"),
            base_events_sha256: crate::sha256_hex(&base_events),
            base_state_sha256: crate::sha256_hex(&base_state),
            events_sha256: crate::sha256_hex(&next_events),
            state_sha256: crate::sha256_hex(state_json(&next_state).as_bytes()),
            sequence: 1,
        };
        let temporary = directory.join(&pending.temp_directory);
        fs::create_dir(&temporary).expect("transaction directory");
        fs::write(temporary.join("events.jsonl"), &next_events).expect("pending events");
        fs::write(
            temporary.join("state.json"),
            state_json(&next_state).as_bytes(),
        )
        .expect("pending state");
        fs::write(directory.join(COMMIT_MARKER), pending_commit_json(&pending))
            .expect("commit marker");

        // Model the observed failure window: event replacement succeeded but
        // state replacement did not. Recovery must finish the same commit.
        fs::write(&event_path, &next_events).expect("partial event replacement");
        let recovered = load_record(&directory).expect("recover pending commit");
        assert_eq!(recovered.state, next_state);
        assert_eq!(
            fs::read(&state_path).expect("recovered state"),
            state_json(&next_state).as_bytes()
        );
        assert!(!directory.join(COMMIT_MARKER).exists());
        assert!(!temporary.exists());
        fs::remove_dir_all(root).expect("remove test root");
    }
}
