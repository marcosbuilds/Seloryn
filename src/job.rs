use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::json::{self, JsonValue};

const JOB_SCHEMA: &str = "seloryn.job.v1";
const EVENT_SCHEMA: &str = "seloryn.job-event.v1";
const MAX_JOB_ID_BYTES: usize = 64;
const MAX_SPEC_BYTES: usize = 4 * 1024 * 1024;
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
}

#[derive(Clone, Debug)]
struct Event {
    sequence: u64,
    job_id: String,
    from: Option<Status>,
    to: Status,
    at: u64,
    spec_sha256: String,
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
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Status => "status",
            Self::Transition => "transition",
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
    };
    let event = Event {
        sequence: 0,
        job_id: job_id.clone(),
        from: None,
        to: Status::Accepted,
        at: now,
        spec_sha256: spec_sha256.clone(),
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
            if metadata.file_type().is_symlink() {
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
    if record.state.status == next {
        return Ok(result_json(
            Operation::Transition,
            &record,
            None,
            Some(next),
        ));
    }
    validate_transition(record.state.status, next)?;
    let now = unix_seconds();
    let event = Event {
        sequence: record.state.sequence.saturating_add(1),
        job_id: job_id.to_owned(),
        from: Some(record.state.status),
        to: next,
        at: now,
        spec_sha256: record.state.spec_sha256.clone(),
    };
    let event_path = directory.join("events.jsonl");
    let mut events = fs::read(&event_path)
        .map_err(|error| JobError::io("SEL-J007", "failed to read the job event log", error))?;
    if !events.ends_with(b"\n") {
        return Err(JobError::new(
            "SEL-J008",
            "job event log does not end at a complete line",
        ));
    }
    events.extend_from_slice(event_line(&event).as_bytes());
    atomic_write(&event_path, &events)?;

    record.state.status = next;
    record.state.sequence = event.sequence;
    record.state.updated_at = now;
    atomic_write(
        &directory.join("state.json"),
        state_json(&record.state).as_bytes(),
    )?;
    let record = load_record_unlocked(&directory)?;
    Ok(result_json(
        Operation::Transition,
        &record,
        None,
        Some(next),
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

fn validate_transition(from: Status, to: Status) -> Result<(), JobError> {
    let allowed = match from {
        Status::Accepted => matches!(to, Status::Queued | Status::Cancelling | Status::Failed),
        Status::Queued => matches!(to, Status::Starting | Status::Cancelling | Status::Failed),
        Status::Starting => {
            matches!(
                to,
                Status::Running | Status::Cancelling | Status::Failed | Status::Lost
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

struct Record {
    state: JobState,
}

struct JobLock {
    _file: File,
}

impl JobLock {
    fn acquire(directory: &Path) -> Result<Self, JobError> {
        let path = directory.join("job.lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| JobError::io("SEL-J010", "failed to open the job lock", error))?;
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
    reject_symlink(directory)?;
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
    let events_path = directory.join("events.jsonl");
    reject_symlink(&events_path)?;
    let events_source = read_required(&events_path, "job event log")?;
    let events = parse_events(&events_source)?;
    if events.is_empty() {
        return Err(JobError::new("SEL-J007", "job event log is empty"));
    }
    let derived = derive_state(&events, &state)?;
    if derived != state {
        atomic_write(&state_path, state_json(&derived).as_bytes())?;
    }
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
    let mut state = JobState {
        job_id: first.job_id.clone(),
        status: first.to,
        sequence: 0,
        attempt: stored.attempt,
        spec_sha256: first.spec_sha256.clone(),
        created_at: first.at,
        updated_at: first.at,
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
        validate_transition(state.status, event.to)?;
        state.status = event.to;
        state.sequence = event.sequence;
        state.updated_at = event.at;
    }
    if stored.job_id != state.job_id
        || stored.attempt != 1
        || stored.created_at != state.created_at
        || stored.spec_sha256 != state.spec_sha256
        || stored.sequence > state.sequence
        || (stored.sequence == state.sequence && stored.status != state.status)
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
    state.attempt = stored.attempt;
    Ok(state)
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

fn state_json(state: &JobState) -> String {
    json::encode(&JsonValue::Object(BTreeMap::from([
        (
            "attempt".to_owned(),
            JsonValue::Number(state.attempt.to_string()),
        ),
        (
            "created_at".to_owned(),
            JsonValue::Number(state.created_at.to_string()),
        ),
        ("job_id".to_owned(), JsonValue::String(state.job_id.clone())),
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
        ("at".to_owned(), JsonValue::Number(event.at.to_string())),
        (
            "from".to_owned(),
            event
                .from
                .map(|status| JsonValue::String(status.as_str().to_owned()))
                .unwrap_or(JsonValue::Null),
        ),
        ("job_id".to_owned(), JsonValue::String(event.job_id.clone())),
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
    JsonValue::Object(BTreeMap::from([
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
    ]))
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
    if !metadata.is_dir() {
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

fn reject_symlink(path: &Path) -> Result<(), JobError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(JobError::new(
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
        || matches!(error.raw_os_error(), Some(145) | Some(183))
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

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), JobError> {
    let parent = path
        .parent()
        .ok_or_else(|| JobError::new("SEL-J010", "cannot determine job storage parent"))?;
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
