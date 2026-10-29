use std::io;

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

    pub(crate) fn parse(value: &str) -> Result<Self, JobError> {
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
pub(crate) struct JobState {
    pub(crate) job_id: String,
    pub(crate) status: Status,
    pub(crate) sequence: u64,
    pub(crate) attempt: u64,
    pub(crate) spec_sha256: String,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) owner: Option<String>,
    pub(crate) attempt_id: Option<String>,
    pub(crate) claimed_at: Option<u64>,
    pub(crate) lease_id: Option<String>,
    pub(crate) lease_expires_at: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct Event {
    pub(crate) sequence: u64,
    pub(crate) job_id: String,
    pub(crate) from: Option<Status>,
    pub(crate) to: Status,
    pub(crate) at: u64,
    pub(crate) spec_sha256: String,
    pub(crate) attempt: Option<u64>,
    pub(crate) kind: String,
    pub(crate) owner: Option<String>,
    pub(crate) attempt_id: Option<String>,
    pub(crate) claimed_at: Option<u64>,
    pub(crate) lease_id: Option<String>,
    pub(crate) lease_expires_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct JobError {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl JobError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub(crate) fn io(code: &'static str, context: &str, error: io::Error) -> Self {
        Self::new(code, format!("{context}: {error}"))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
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
    pub(crate) fn as_str(self) -> &'static str {
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
