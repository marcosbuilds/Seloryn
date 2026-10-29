use super::*;

pub(super) const JOB_SCHEMA: &str = "seloryn.job.v1";
const EVENT_SCHEMA: &str = "seloryn.job-event.v1";
const COMMIT_SCHEMA: &str = "seloryn.job-commit.v1";
pub(super) const COMMIT_MARKER: &str = ".job-commit.json";
pub(super) const COMMIT_DIRECTORY_PREFIX: &str = ".job-commit-tmp-";
#[allow(dead_code)]
pub(super) const MAX_RESULT_BYTES: usize = 1024 * 1024;
const LOCK_WAIT_ATTEMPTS: usize = 500;
const LOCK_WAIT_STEP: Duration = Duration::from_millis(10);
pub(super) fn commit_event(
    directory: &Path,
    record: &mut Record,
    event: Event,
) -> Result<Record, JobError> {
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
        base_events_sha256: sha256_hex(&events),
        base_state_sha256: sha256_hex(&stored_state),
        events_sha256: sha256_hex(&next_events),
        state_sha256: sha256_hex(state_json(&next_state).as_bytes()),
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
pub(super) struct PendingCommit {
    pub(super) temp_directory: String,
    pub(super) base_events_sha256: String,
    pub(super) base_state_sha256: String,
    pub(super) events_sha256: String,
    pub(super) state_sha256: String,
    pub(super) sequence: u64,
}

pub(super) fn apply_event(state: &mut JobState, event: &Event) {
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

pub(super) fn pending_commit_json(commit: &PendingCommit) -> String {
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

pub(super) fn recover_pending_commit(directory: &Path) -> Result<(), JobError> {
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
    let current_events_sha256 = sha256_hex(&current_events);
    let current_state_sha256 = sha256_hex(&current_state);
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
        if sha256_hex(&events) != pending.events_sha256
            || sha256_hex(&state) != pending.state_sha256
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
    if sha256_hex(&final_events) != pending.events_sha256
        || sha256_hex(&final_state) != pending.state_sha256
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

pub(super) struct Record {
    pub(super) state: JobState,
}

pub(super) struct JobLock {
    _file: File,
}

impl JobLock {
    pub(super) fn acquire(directory: &Path) -> Result<Self, JobError> {
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

pub(super) fn load_record(directory: &Path) -> Result<Record, JobError> {
    let _lock = JobLock::acquire(directory)?;
    load_record_unlocked(directory)
}

pub(super) fn load_record_unlocked(directory: &Path) -> Result<Record, JobError> {
    let record = load_record_unlocked_read_only(directory)?;
    let state_path = directory.join("state.json");
    let state_source = read_required(&state_path, "job state")?;
    let stored = parse_state(&state_source)?;
    if record.state != stored {
        atomic_write(&state_path, state_json(&record.state).as_bytes())?;
    }
    Ok(record)
}

pub(super) fn load_record_unlocked_read_only(directory: &Path) -> Result<Record, JobError> {
    load_record_unlocked_read_only_with_spec(directory, true)
}

pub(super) fn load_record_unlocked_read_only_without_spec(
    directory: &Path,
) -> Result<Record, JobError> {
    load_record_unlocked_read_only_with_spec(directory, false)
}

pub(super) fn load_record_unlocked_read_only_with_spec(
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
            sha256_hex(format!("{}\n", json::encode(&spec_value)).as_bytes());
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

pub(super) fn parse_state(source: &str) -> Result<JobState, JobError> {
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

pub(super) fn parse_events(source: &str) -> Result<Vec<Event>, JobError> {
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

pub(super) fn object(
    value: JsonValue,
    name: &str,
) -> Result<BTreeMap<String, JsonValue>, JobError> {
    match value {
        JsonValue::Object(object) => Ok(object),
        _ => Err(JobError::new(
            "SEL-J007",
            format!("{name} must be a JSON object"),
        )),
    }
}

pub(super) fn object_string_from<'a>(
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

pub(super) fn object_u64(object: &BTreeMap<String, JsonValue>, key: &str) -> Result<u64, JobError> {
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

pub(super) fn object_string<'a>(value: &'a JsonValue, key: &str) -> Option<&'a str> {
    match value {
        JsonValue::Object(object) => match object.get(key) {
            Some(JsonValue::String(value)) => Some(value),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn object_optional_string<'a>(
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

pub(super) fn object_optional_u64(
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

pub(super) fn state_json(state: &JobState) -> String {
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

pub(super) fn event_line(event: &Event) -> String {
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

pub(super) fn job_summary(record: &Record) -> JsonValue {
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

pub(super) fn archive_result(
    directory: &Path,
    attempt: u64,
    spec_sha256: &str,
    require_committed: bool,
) -> Result<(), JobError> {
    let source = directory.join("result.json");
    reject_symlink(&source)?;
    let bytes = match fs::read(&source) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if require_committed {
                return Err(JobError::new(
                    "SEL-J019",
                    "terminal job has no committed result receipt",
                ));
            }
            return Ok(());
        }
        Err(error) => {
            return Err(JobError::io(
                "SEL-J010",
                "failed to read the previous job result",
                error,
            ));
        }
    };
    if bytes.len() > MAX_RESULT_BYTES {
        return Err(JobError::new(
            "SEL-J002",
            "job result receipt exceeds the byte limit",
        ));
    }
    let source_text = String::from_utf8(bytes.clone())
        .map_err(|_| JobError::new("SEL-J007", "job result receipt must be UTF-8 JSON"))?;
    let value = json::parse(&source_text, MAX_RESULT_BYTES, MAX_JSON_DEPTH).map_err(|error| {
        JobError::new("SEL-J007", format!("invalid job result: {}", error.message))
    })?;
    let object = object(value, "job result")?;
    if object_string_from(&object, "schema")? != "seloryn.job-result.v1"
        || object_string_from(&object, "spec_sha256")? != spec_sha256
        || object_u64(&object, "attempt")? != attempt
    {
        return Err(JobError::new(
            "SEL-J019",
            "job result receipt does not match the terminal attempt",
        ));
    }
    let committed = matches!(object.get("committed"), Some(JsonValue::Bool(true)));
    if require_committed && !committed {
        return Err(JobError::new(
            "SEL-J019",
            "terminal job result receipt is not committed",
        ));
    }
    let attempts = directory.join("attempts");
    ensure_directory(&attempts, "job attempt archive")?;
    let destination = attempts.join(format!("attempt-{attempt}.json"));
    reject_symlink(&destination)?;
    match fs::read(&destination) {
        Ok(existing) if existing == bytes => Ok(()),
        Ok(_) => Err(JobError::new(
            "SEL-J019",
            "job attempt archive already contains different receipt bytes",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => atomic_write(&destination, &bytes),
        Err(error) => Err(JobError::io(
            "SEL-J010",
            "failed to inspect the previous job attempt archive",
            error,
        )),
    }
}

pub(super) fn result_json(
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

pub(super) fn read_required(path: &Path, name: &str) -> Result<String, JobError> {
    fs::read_to_string(path)
        .map_err(|error| JobError::io("SEL-J007", &format!("failed to read {name}"), error))
}

pub(super) fn jobs_directory(root: &Path) -> PathBuf {
    root.join(".seloryn").join("jobs")
}

pub(super) fn job_directory(root: &Path, job_id: &str) -> PathBuf {
    jobs_directory(root).join(job_id)
}

pub(super) fn ensure_root_directory(root: &Path) -> Result<(), JobError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| JobError::io("SEL-J007", "failed to inspect the job root", error))?;
    if metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(JobError::new("SEL-J007", "job root is not a directory"));
    }
    Ok(())
}

pub(super) fn ensure_directory(path: &Path, name: &str) -> Result<(), JobError> {
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

pub(super) fn ensure_store_contained(root: &Path, jobs_root: &Path) -> Result<(), JobError> {
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

pub(super) fn ensure_job_contained(root: &Path, directory: &Path) -> Result<(), JobError> {
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

pub(super) fn metadata_is_reparse(metadata: &fs::Metadata) -> bool {
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

pub(super) fn reject_symlink(path: &Path) -> Result<(), JobError> {
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

pub(super) fn is_existing_directory_error(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::AlreadyExists
        // POSIX reports a non-empty directory target as ENOTEMPTY when the
        // atomic publish races with or reuses an existing job directory.
        || matches!(error.raw_os_error(), Some(39) | Some(66) | Some(145) | Some(183))
}

pub(super) fn generated_job_id() -> String {
    let entropy = format!(
        "{}:{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default(),
        env!("CARGO_PKG_VERSION"),
    );
    format!("job-{}", &sha256_hex(entropy.as_bytes())[..24])
}

pub(super) fn generated_owner() -> String {
    format!("cli-{}", std::process::id())
}

pub(super) fn generated_attempt_id(owner: &str) -> String {
    let entropy = format!(
        "{}:{}:{}:{}",
        owner,
        std::process::id(),
        unix_nanos(),
        env!("CARGO_PKG_VERSION"),
    );
    format!("attempt-{}", &sha256_hex(entropy.as_bytes())[..24])
}

pub(super) fn generated_lease_id(owner: &str, attempt_id: &str) -> String {
    let entropy = format!(
        "{}:{}:{}:{}",
        owner,
        attempt_id,
        unix_nanos(),
        env!("CARGO_PKG_VERSION"),
    );
    format!("lease-{}", &sha256_hex(entropy.as_bytes())[..32])
}

pub(super) fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

pub(super) fn lease_deadline(ttl_seconds: u64) -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| lease_deadline_from_duration(duration, ttl_seconds))
        .unwrap_or(ttl_seconds)
}

pub(super) fn lease_deadline_from_duration(duration: Duration, ttl_seconds: u64) -> u64 {
    duration
        .as_secs()
        .saturating_add(u64::from(duration.subsec_nanos() != 0))
        .saturating_add(ttl_seconds)
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), JobError> {
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

pub(super) fn unix_nanos() -> u128 {
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

pub(super) fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}
