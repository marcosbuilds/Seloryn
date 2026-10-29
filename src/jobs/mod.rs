use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

mod contract;
pub(crate) mod job_worker;
mod store;

use crate::data::sha256_hex;
use crate::json::{self, JsonValue};
pub(crate) use contract::{Event, JobError, JobState, Operation, Status};
use store::{
    JOB_SCHEMA, JobLock, MAX_RESULT_BYTES, Record, archive_result, atomic_write, commit_event,
    ensure_directory, ensure_job_contained, ensure_root_directory, ensure_store_contained,
    event_line, generated_attempt_id, generated_job_id, generated_lease_id, generated_owner,
    is_existing_directory_error, job_directory, job_summary, jobs_directory, lease_deadline,
    load_record, load_record_unlocked, load_record_unlocked_read_only_without_spec,
    metadata_is_reparse, object, object_string, object_string_from, object_u64, parse_events,
    read_required, reject_symlink, result_json, state_json, sync_directory, unix_nanos,
    unix_seconds,
};

const MAX_JOB_ID_BYTES: usize = 64;
const MAX_SPEC_BYTES: usize = 4 * 1024 * 1024;
const MAX_PREFLIGHT_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSON_DEPTH: usize = 64;

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
    let spec_sha256 = sha256_hex(canonical_spec.as_bytes());
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
    let actual_spec_sha256 = sha256_hex(canonical_spec.as_bytes());
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
    let source_sha256 = sha256_hex(&source_bytes);
    if source_sha256 != spec.source_sha256 {
        return Err(preflight_error(
            "source_sha256 does not match source content",
        ));
    }
    let parsed = crate::language::parser::parse(&source);
    if let Some(diagnostic) = parsed.diagnostics.first() {
        return Err(source_diagnostic_error(diagnostic));
    }
    let program = parsed
        .program
        .ok_or_else(|| preflight_error("source parser produced no program"))?;
    if let Some(diagnostic) = crate::language::semantic::check(&program).first() {
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
    archive_result(
        &directory,
        record.state.attempt,
        &record.state.spec_sha256,
        matches!(record.state.status, Status::Failed | Status::Cancelled),
    )?;
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
        Status::Lost => matches!(to, Status::RetryWait | Status::Failed),
        Status::RetryWait => matches!(to, Status::Queued | Status::Failed),
        Status::Succeeded | Status::Cancelled | Status::Failed => false,
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

#[cfg(test)]
mod tests {
    use super::store::{
        COMMIT_DIRECTORY_PREFIX, COMMIT_MARKER, PendingCommit, apply_event,
        lease_deadline_from_duration, pending_commit_json,
    };
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
            base_events_sha256: sha256_hex(&base_events),
            base_state_sha256: sha256_hex(&base_state),
            events_sha256: sha256_hex(&next_events),
            state_sha256: sha256_hex(state_json(&next_state).as_bytes()),
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
