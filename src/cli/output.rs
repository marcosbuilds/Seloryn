use super::*;

#[derive(Clone, Debug)]
pub(super) struct RunIdentity {
    pub(super) program_sha256: String,
    pub(super) manifest_sha256: Option<String>,
}

const EVENT_QUEUE_CAPACITY: usize = 32;

struct AsyncLogWriter {
    sender: Option<SyncSender<String>>,
    failure: Arc<Mutex<Option<String>>>,
    handle: Option<JoinHandle<()>>,
}

impl AsyncLogWriter {
    fn new() -> Self {
        let (sender, receiver) = sync_channel::<String>(EVENT_QUEUE_CAPACITY);
        let failure = Arc::new(Mutex::new(None));
        let thread_failure = Arc::clone(&failure);
        let handle = thread::spawn(move || {
            let mut stderr = io::stderr();
            for line in receiver {
                if let Err(error) = stderr
                    .write_all(line.as_bytes())
                    .and_then(|()| stderr.flush())
                {
                    if let Ok(mut slot) = thread_failure.lock() {
                        *slot = Some(error.to_string());
                    }
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            failure,
            handle: Some(handle),
        }
    }

    fn send(&self, line: String) -> io::Result<()> {
        if let Ok(slot) = self.failure.lock()
            && let Some(message) = slot.as_ref()
        {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, message.clone()));
        }
        self.sender
            .as_ref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "event writer is closed"))?
            .send(line)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "event writer disconnected"))
    }

    fn finish(&mut self) -> Option<io::Error> {
        self.sender.take();
        if let Some(handle) = self.handle.take()
            && handle.join().is_err()
        {
            return Some(io::Error::other("event writer thread panicked"));
        }
        self.failure
            .lock()
            .ok()
            .and_then(|slot| slot.clone())
            .map(|message| io::Error::new(io::ErrorKind::BrokenPipe, message))
    }
}

pub(super) struct CliEventSink {
    format: LogFormat,
    failure: Option<io::Error>,
    cancellation_observed: bool,
    pub(super) identity: Option<RunIdentity>,
    attempt_id: String,
    signing_key: Option<Vec<u8>>,
    result_sha256: Option<String>,
    checkpoint_after_sha256: Option<String>,
    writer: Option<AsyncLogWriter>,
    v2_run: Option<String>,
    v2_sequence: u64,
    v2_previous_hash: String,
}

impl CliEventSink {
    pub(super) fn new(format: LogFormat, identity: Option<RunIdentity>) -> Self {
        Self {
            format,
            failure: None,
            cancellation_observed: false,
            identity,
            attempt_id: receipt_attempt_id(),
            signing_key: (format == LogFormat::Json2Signed)
                .then(|| env::var("SELORYN_RECEIPT_KEY").ok())
                .flatten()
                .filter(|key| !key.is_empty())
                .map(|key| key.into_bytes()),
            result_sha256: None,
            checkpoint_after_sha256: None,
            writer: (!matches!(format, LogFormat::Off)).then(AsyncLogWriter::new),
            v2_run: None,
            v2_sequence: 0,
            v2_previous_hash: v2_zero_hash(),
        }
    }

    pub(super) fn take_failure(&mut self) -> Option<io::Error> {
        if let Some(writer) = &mut self.writer
            && let Some(error) = writer.finish()
            && self.failure.is_none()
        {
            self.failure = Some(error);
        }
        self.failure.take()
    }

    pub(super) fn has_run(&self) -> bool {
        self.v2_run.is_some()
    }

    pub(super) fn was_cancelled(&self) -> bool {
        self.cancellation_observed
    }

    pub(super) fn set_result_sha256(&mut self, value: String) {
        self.result_sha256 = Some(value);
    }

    fn ensure_run_started(
        &mut self,
        source_event: &str,
        run_fingerprint: String,
        mut identity: BTreeMap<String, json::JsonValue>,
    ) {
        if !is_receipt_log(self.format) || self.v2_run.is_some() {
            return;
        }
        identity.insert(
            "schema".to_owned(),
            json::JsonValue::String("seloryn.run-identity.v1".to_owned()),
        );
        let plan = json::JsonValue::Object(BTreeMap::from([
            (
                "identity".to_owned(),
                json::JsonValue::Object(identity.clone()),
            ),
            (
                "source_event".to_owned(),
                json::JsonValue::String(source_event.to_owned()),
            ),
            (
                "run_fingerprint".to_owned(),
                json::JsonValue::String(run_fingerprint.clone()),
            ),
        ]));
        let plan_hash = v2_hash(&plan);
        self.v2_run = Some(format!("r_{}", &plan_hash["sha256:".len()..]));
        let data = BTreeMap::from([
            (
                "attempt_id".to_owned(),
                json::JsonValue::String(self.attempt_id.clone()),
            ),
            ("identity".to_owned(), json::JsonValue::Object(identity)),
            (
                "language_version".to_owned(),
                json::JsonValue::String(LANGUAGE_VERSION.to_owned()),
            ),
            (
                "run_fingerprint".to_owned(),
                json::JsonValue::String(run_fingerprint),
            ),
            (
                "runtime_package".to_owned(),
                json::JsonValue::String(PACKAGE_VERSION.to_owned()),
            ),
            (
                "source_event".to_owned(),
                json::JsonValue::String(source_event.to_owned()),
            ),
        ]);
        self.write_v2_record("run.start", json::JsonValue::Object(data));
    }

    pub(super) fn record_preflight_error(&mut self, code: &str, message: &str) {
        if !is_receipt_log(self.format) {
            return;
        }
        let data = BTreeMap::from([
            (
                "error_code".to_owned(),
                json::JsonValue::String(code.to_owned()),
            ),
            (
                "error_message_sha256".to_owned(),
                json::JsonValue::String(sha256_hex(message.as_bytes())),
            ),
        ]);
        let fingerprint = v2_hash(&json::JsonValue::Object(data.clone()));
        self.ensure_run_started("preflight", fingerprint, BTreeMap::new());
        self.write_v2_record("preflight.error", json::JsonValue::Object(data));
    }

    fn emit_result(&self) -> Result<(), String> {
        self.failure
            .as_ref()
            .map(|error| Err(error.to_string()))
            .unwrap_or(Ok(()))
    }

    fn write_line(&mut self, line: String) {
        if self.failure.is_some() || self.format == LogFormat::Off {
            return;
        }
        let line = format!("{line}\n");
        if let Some(writer) = &self.writer {
            if let Err(error) = writer.send(line) {
                self.failure = Some(error);
            }
        } else {
            self.failure = Some(io::Error::other("event writer is unavailable"));
        }
    }

    pub(super) fn finish(
        &mut self,
        status: &str,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) {
        if !is_receipt_log(self.format) || self.v2_run.is_none() || self.failure.is_some() {
            return;
        }
        let mut data = BTreeMap::new();
        data.insert(
            "status".to_owned(),
            json::JsonValue::String(status.to_owned()),
        );
        data.insert(
            "events_before_end".to_owned(),
            json::JsonValue::Number(self.v2_sequence.to_string()),
        );
        data.insert(
            "last_event_hash".to_owned(),
            json::JsonValue::String(self.v2_previous_hash.clone()),
        );
        if let Some(error_code) = error_code {
            data.insert(
                "error_code".to_owned(),
                json::JsonValue::String(error_code.to_owned()),
            );
        }
        if let Some(error_message) = error_message {
            data.insert(
                "error_message".to_owned(),
                json::JsonValue::String(error_message.to_owned()),
            );
        }
        if let Some(result_sha256) = &self.result_sha256 {
            data.insert(
                "result_sha256".to_owned(),
                json::JsonValue::String(result_sha256.clone()),
            );
        }
        if let Some(checkpoint_sha256) = &self.checkpoint_after_sha256 {
            data.insert(
                "checkpoint_after_sha256".to_owned(),
                json::JsonValue::String(checkpoint_sha256.clone()),
            );
        }
        self.write_v2_record("run.end", json::JsonValue::Object(data));
    }

    fn write_v2_record(&mut self, kind: &str, data: json::JsonValue) {
        if self.format == LogFormat::Json2Signed && self.signing_key.is_none() {
            self.failure = Some(io::Error::new(
                io::ErrorKind::InvalidInput,
                "SEL-C006 json2-signed requires non-empty SELORYN_RECEIPT_KEY",
            ));
            return;
        }
        let Some(run) = self.v2_run.clone() else {
            return;
        };
        let mut record = BTreeMap::new();
        record.insert("data".to_owned(), data);
        record.insert("kind".to_owned(), json::JsonValue::String(kind.to_owned()));
        record.insert(
            "phase".to_owned(),
            json::JsonValue::String("run".to_owned()),
        );
        record.insert(
            "prev".to_owned(),
            json::JsonValue::String(self.v2_previous_hash.clone()),
        );
        record.insert("run".to_owned(), json::JsonValue::String(run));
        record.insert("schema".to_owned(), json::JsonValue::Number("2".to_owned()));
        record.insert(
            "seq".to_owned(),
            json::JsonValue::Number(self.v2_sequence.to_string()),
        );
        let hash = v2_hash(&json::JsonValue::Object(record.clone()));
        record.insert("hash".to_owned(), json::JsonValue::String(hash.clone()));
        if let Some(key) = &self.signing_key {
            record.insert(
                "signature".to_owned(),
                json::JsonValue::String(hmac_sha256(key, hash.as_bytes())),
            );
        }
        self.write_line(json::encode(&json::JsonValue::Object(record)));
        if self.failure.is_none() {
            self.v2_sequence = self.v2_sequence.saturating_add(1);
            self.v2_previous_hash = hash;
        }
    }
}

impl runtime::EventSink for CliEventSink {
    fn emit(&mut self, event: runtime::RuntimeEvent) -> Result<(), String> {
        let cancellation_was_requested = cancellation_requested();
        if cancellation_was_requested && !is_receipt_log(self.format) {
            self.cancellation_observed = true;
            return Err(CANCELLATION_MESSAGE.to_owned());
        }
        let line = match self.format {
            LogFormat::Off => return Ok(()),
            LogFormat::Human => render_runtime_event_human(&event),
            LogFormat::Json => render_runtime_event_json(&event),
            LogFormat::Json2 | LogFormat::Json2Signed => {
                let source = render_runtime_event_json(&event);
                let Ok(json::JsonValue::Object(mut object)) =
                    json::parse(&source, source.len(), 16)
                else {
                    self.failure = Some(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "failed to convert runtime event to streaming v2",
                    ));
                    return self.emit_result();
                };
                let kind = match object.remove("event") {
                    Some(json::JsonValue::String(kind)) => kind,
                    _ => {
                        self.failure = Some(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "runtime event has no event kind",
                        ));
                        return self.emit_result();
                    }
                };
                object.remove("schema_version");
                redact_v2_paths(&mut object);
                let fingerprint = v2_hash(&json::JsonValue::Object(object.clone()));
                let mut identity = BTreeMap::new();
                if let Some(run_identity) = &self.identity {
                    identity.insert(
                        "program_sha256".to_owned(),
                        json::JsonValue::String(run_identity.program_sha256.clone()),
                    );
                    if let Some(manifest_sha256) = &run_identity.manifest_sha256 {
                        identity.insert(
                            "manifest_sha256".to_owned(),
                            json::JsonValue::String(manifest_sha256.clone()),
                        );
                    }
                }
                for key in ["train_sha256", "test_sha256", "checkpoint_before_sha256"] {
                    if let Some(value) = object.get(key) {
                        identity.insert(key.to_owned(), value.clone());
                    }
                }
                if let Some(json::JsonValue::Object(payload)) = object.get("payload") {
                    for key in ["train_sha256", "test_sha256", "checkpoint_before_sha256"] {
                        if let Some(value) = payload.get(key) {
                            identity.insert(key.to_owned(), value.clone());
                        }
                    }
                }
                self.ensure_run_started(&kind, fingerprint, identity);
                let audit_kind_is_training_complete = matches!(
                    object.get("audit_kind"),
                    Some(json::JsonValue::String(audit_kind))
                        if audit_kind == "training.complete"
                );
                if kind == "training.complete" || audit_kind_is_training_complete {
                    let checkpoint = object.get("checkpoint_sha256").or_else(|| {
                        let Some(json::JsonValue::Object(payload)) = object.get("payload") else {
                            return None;
                        };
                        payload.get("checkpoint_sha256")
                    });
                    if let Some(json::JsonValue::String(checkpoint_sha256)) = checkpoint {
                        self.checkpoint_after_sha256 = Some(checkpoint_sha256.clone());
                    }
                }
                self.write_v2_record(&kind, json::JsonValue::Object(object));
                if cancellation_was_requested {
                    self.cancellation_observed = true;
                    return Err(CANCELLATION_MESSAGE.to_owned());
                }
                return self.emit_result();
            }
        };
        self.write_line(line);
        self.emit_result()
    }
}

fn v2_zero_hash() -> String {
    format!("sha256:{}", "0".repeat(64))
}

fn v2_hash(value: &json::JsonValue) -> String {
    let mut hasher = Sha256::new();
    hasher.update(json::encode(value).as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub(super) fn redact_sensitive_json(value: &mut json::JsonValue) {
    match value {
        json::JsonValue::Object(object) => {
            for (key, value) in object.iter_mut() {
                let normalized = key.to_ascii_lowercase();
                if normalized == "token"
                    || normalized.ends_with("_token")
                    || normalized.contains("password")
                    || normalized.contains("secret")
                    || normalized.contains("api_key")
                    || normalized.contains("authorization")
                    || normalized.contains("credential")
                    || normalized.contains("private_key")
                {
                    *value = json::JsonValue::String("<redacted>".to_owned());
                } else {
                    redact_sensitive_json(value);
                }
            }
        }
        json::JsonValue::Array(values) => {
            for value in values {
                redact_sensitive_json(value);
            }
        }
        json::JsonValue::Null
        | json::JsonValue::Bool(_)
        | json::JsonValue::Number(_)
        | json::JsonValue::String(_) => {}
    }
}

fn redact_v2_paths(object: &mut BTreeMap<String, json::JsonValue>) {
    for key in ["train_path", "test_path", "checkpoint_path", "input_path"] {
        let Some(json::JsonValue::String(path)) = object.get(key).cloned() else {
            continue;
        };
        object.insert(
            key.to_owned(),
            json::JsonValue::String(format!("path_sha256:{}", sha256_hex(path.as_bytes()))),
        );
    }
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts arbitrary keys");
    mac.update(message);
    format!("hmac-sha256:{:x}", mac.finalize().into_bytes())
}

fn receipt_attempt_id() -> String {
    let entropy = format!(
        "{}:{}:{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default(),
        PACKAGE_VERSION,
    );
    format!("a_{}", &sha256_hex(entropy.as_bytes())[..32])
}

pub(super) fn is_receipt_log(format: LogFormat) -> bool {
    matches!(format, LogFormat::Json2 | LogFormat::Json2Signed)
}

fn render_runtime_event_human(event: &runtime::RuntimeEvent) -> String {
    match event {
        runtime::RuntimeEvent::Started {
            model,
            train_path,
            test_path,
            train_sha256: _,
            test_sha256: _,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
        } => format!(
            "seloryn training.start schema=1 operation=train_linear model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} train={} test={} train_samples={} test_samples={} steps={} seed_metadata={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            json::encode(&json::JsonValue::String(train_path.clone())),
            json::encode(&json::JsonValue::String(test_path.clone())),
            train_samples,
            test_samples,
            steps,
            seed,
        ),
        runtime::RuntimeEvent::Step {
            model,
            step,
            steps,
            updates,
            loss,
            loss_delta,
        } => format!(
            "seloryn training.step schema=1 model={} step={}/{} updates={} loss={loss:.8} loss_delta={loss_delta:.8}",
            model, step, steps, updates
        ),
        runtime::RuntimeEvent::Completed {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            initial_loss,
            final_loss,
            loss_delta,
            loss_decreased,
            holdout_mae_observed,
            weight,
            bias,
        } => format!(
            "seloryn training.complete schema=1 operation=train_linear model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} updates={} steps={} initial_loss={initial_loss:.8} final_loss={final_loss:.8} loss_delta={loss_delta:.8} loss_decreased={} holdout_mae_observed={holdout_mae_observed:.8} weight={weight:.8} bias={bias:.8} seed_metadata={}",
            model, device, dtype, optimizer, parameter_count, updates, steps, loss_decreased, seed
        ),
        runtime::RuntimeEvent::GpuStarted {
            model,
            train_path,
            test_path,
            train_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            adapter,
            backend,
            ..
        } => format!(
            "seloryn training.start schema=1 operation=train_linear_gpu model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} train={} test={} train_samples={} steps={} seed={} adapter={} backend={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            json::encode(&json::JsonValue::String(train_path.clone())),
            json::encode(&json::JsonValue::String(test_path.clone())),
            train_samples,
            steps,
            seed,
            adapter,
            backend,
        ),
        runtime::RuntimeEvent::GpuCompleted {
            model,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            initial_loss,
            final_loss,
            loss_decreased,
            holdout_mae_observed,
            weight,
            bias,
            adapter,
            backend,
            dispatches,
            readbacks,
        } => format!(
            "seloryn training.complete schema=1 operation=train_linear_gpu model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} steps={} initial_loss={initial_loss:.8} final_loss={final_loss:.8} loss_decreased={} holdout_mae_observed={holdout_mae_observed:.8} weight={weight:.8} bias={bias:.8} seed={} adapter={} backend={} dispatches={} readbacks={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            steps,
            loss_decreased,
            seed,
            adapter,
            backend,
            dispatches,
            readbacks,
        ),
        runtime::RuntimeEvent::MlpStarted {
            model,
            train_path,
            test_path,
            checkpoint_path,
            train_sha256: _,
            test_sha256: _,
            checkpoint_before_sha256: _,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            resumed_from,
        } => format!(
            "seloryn training.start schema=1 operation=train_mlp model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} batch_size={} hidden_size={} train={} test={} checkpoint={} train_samples={} test_samples={} steps={} seed={} resumed_from={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            batch_size,
            hidden_size,
            json::encode(&json::JsonValue::String(train_path.clone())),
            json::encode(&json::JsonValue::String(test_path.clone())),
            json::encode(&json::JsonValue::String(checkpoint_path.clone())),
            train_samples,
            test_samples,
            steps,
            seed,
            resumed_from,
        ),
        runtime::RuntimeEvent::MlpStep {
            model,
            step,
            steps,
            updates,
            epoch,
            loss,
            evaluation_loss,
        } => format!(
            "seloryn training.step schema=1 operation=train_mlp model={} step={}/{} updates={} epoch={} loss={loss:.8} evaluation_loss={evaluation_loss:.8}",
            model, step, steps, updates, epoch
        ),
        runtime::RuntimeEvent::MlpCompleted {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            initial_loss,
            final_loss,
            evaluation_loss,
            loss_decreased,
            resumed_from,
            checkpoint_sha256,
        } => format!(
            "seloryn training.complete schema=1 operation=train_mlp model={} device={} dtype={} optimizer={} learning_rate={learning_rate:.8} parameter_count={} batch_size={} hidden_size={} updates={} steps={} initial_loss={initial_loss:.8} final_loss={final_loss:.8} evaluation_loss={evaluation_loss:.8} loss_decreased={} seed={} resumed_from={} checkpoint_sha256={}",
            model,
            device,
            dtype,
            optimizer,
            parameter_count,
            batch_size,
            hidden_size,
            updates,
            steps,
            loss_decreased,
            seed,
            resumed_from,
            checkpoint_sha256,
        ),
        runtime::RuntimeEvent::InferenceStarted {
            model,
            checkpoint_path,
            input_path,
            samples,
            input_size,
            output_size,
            checkpoint_step,
            ..
        } => format!(
            "seloryn inference.start schema=1 operation=predict_mlp model={} device=cpu dtype=f32 checkpoint={} input={} samples={} input_size={} output_size={} checkpoint_step={}",
            model,
            json::encode(&json::JsonValue::String(checkpoint_path.clone())),
            json::encode(&json::JsonValue::String(input_path.clone())),
            samples,
            input_size,
            output_size,
            checkpoint_step,
        ),
        runtime::RuntimeEvent::InferenceCompleted {
            model,
            samples,
            input_size,
            output_size,
            checkpoint_step,
            ..
        } => format!(
            "seloryn inference.complete schema=1 operation=predict_mlp model={} device=cpu dtype=f32 samples={} input_size={} output_size={} checkpoint_step={}",
            model, samples, input_size, output_size, checkpoint_step
        ),
        runtime::RuntimeEvent::Progress {
            operation,
            step,
            total,
            metric,
        } => format!(
            "seloryn training.progress schema=1 operation={} step={}/{} metric={metric:.8}",
            operation, step, total
        ),
        runtime::RuntimeEvent::Audit { kind, payload } => {
            let mut safe_payload = payload.clone();
            redact_sensitive_json(&mut safe_payload);
            format!(
                "seloryn audit.record schema=1 kind={} payload={}",
                kind,
                json::encode(&safe_payload)
            )
        }
    }
}

fn render_runtime_event_json(event: &runtime::RuntimeEvent) -> String {
    let mut object = BTreeMap::new();
    match event {
        runtime::RuntimeEvent::Started {
            model,
            train_path,
            test_path,
            train_sha256,
            test_sha256,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.start".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_linear".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "train_path".to_owned(),
                json::JsonValue::String(train_path.clone()),
            );
            object.insert(
                "test_path".to_owned(),
                json::JsonValue::String(test_path.clone()),
            );
            object.insert(
                "train_sha256".to_owned(),
                json::JsonValue::String(train_sha256.clone()),
            );
            object.insert(
                "test_sha256".to_owned(),
                json::JsonValue::String(test_sha256.clone()),
            );
            object.insert(
                "train_samples".to_owned(),
                json::JsonValue::Number(train_samples.to_string()),
            );
            object.insert(
                "test_samples".to_owned(),
                json::JsonValue::Number(test_samples.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "seed_semantics".to_owned(),
                json::JsonValue::String("metadata_only".to_owned()),
            );
            object.insert(
                "step_sampling".to_owned(),
                json::JsonValue::String("first,10_percent,final".to_owned()),
            );
        }
        runtime::RuntimeEvent::Step {
            model,
            step,
            steps,
            updates,
            loss,
            loss_delta,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.step".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("step".to_owned(), json::JsonValue::Number(step.to_string()));
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert(
                "loss".to_owned(),
                json::JsonValue::Number(format!("{loss:.8}")),
            );
            object.insert(
                "loss_delta".to_owned(),
                json::JsonValue::Number(format!("{loss_delta:.8}")),
            );
        }
        runtime::RuntimeEvent::Completed {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            initial_loss,
            final_loss,
            loss_delta,
            loss_decreased,
            holdout_mae_observed,
            weight,
            bias,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.complete".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_linear".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "seed_semantics".to_owned(),
                json::JsonValue::String("metadata_only".to_owned()),
            );
            object.insert(
                "initial_loss".to_owned(),
                json::JsonValue::Number(format!("{initial_loss:.8}")),
            );
            object.insert(
                "final_loss".to_owned(),
                json::JsonValue::Number(format!("{final_loss:.8}")),
            );
            object.insert(
                "loss_delta".to_owned(),
                json::JsonValue::Number(format!("{loss_delta:.8}")),
            );
            object.insert(
                "loss_decreased".to_owned(),
                json::JsonValue::Bool(*loss_decreased),
            );
            object.insert(
                "holdout_mae_observed".to_owned(),
                json::JsonValue::Number(format!("{holdout_mae_observed:.8}")),
            );
            object.insert(
                "weight".to_owned(),
                json::JsonValue::Number(format!("{weight:.8}")),
            );
            object.insert(
                "bias".to_owned(),
                json::JsonValue::Number(format!("{bias:.8}")),
            );
        }
        runtime::RuntimeEvent::GpuStarted {
            model,
            train_path,
            test_path,
            train_sha256,
            test_sha256,
            train_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            adapter,
            backend,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.start".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_linear_gpu".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert(
                "train_path".to_owned(),
                json::JsonValue::String(train_path.clone()),
            );
            object.insert(
                "test_path".to_owned(),
                json::JsonValue::String(test_path.clone()),
            );
            object.insert(
                "train_sha256".to_owned(),
                json::JsonValue::String(train_sha256.clone()),
            );
            object.insert(
                "test_sha256".to_owned(),
                json::JsonValue::String(test_sha256.clone()),
            );
            object.insert(
                "train_samples".to_owned(),
                json::JsonValue::Number(train_samples.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "adapter".to_owned(),
                json::JsonValue::String(adapter.clone()),
            );
            object.insert(
                "backend".to_owned(),
                json::JsonValue::String(backend.clone()),
            );
        }
        runtime::RuntimeEvent::GpuCompleted {
            model,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            parameter_count,
            initial_loss,
            final_loss,
            loss_decreased,
            holdout_mae_observed,
            weight,
            bias,
            adapter,
            backend,
            dispatches,
            readbacks,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.complete".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_linear_gpu".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "initial_loss".to_owned(),
                json::JsonValue::Number(format!("{initial_loss:.8}")),
            );
            object.insert(
                "final_loss".to_owned(),
                json::JsonValue::Number(format!("{final_loss:.8}")),
            );
            object.insert(
                "loss_decreased".to_owned(),
                json::JsonValue::Bool(*loss_decreased),
            );
            object.insert(
                "holdout_mae_observed".to_owned(),
                json::JsonValue::Number(format!("{holdout_mae_observed:.8}")),
            );
            object.insert(
                "weight".to_owned(),
                json::JsonValue::Number(format!("{weight:.8}")),
            );
            object.insert(
                "bias".to_owned(),
                json::JsonValue::Number(format!("{bias:.8}")),
            );
            object.insert(
                "adapter".to_owned(),
                json::JsonValue::String(adapter.clone()),
            );
            object.insert(
                "backend".to_owned(),
                json::JsonValue::String(backend.clone()),
            );
            object.insert(
                "dispatches".to_owned(),
                json::JsonValue::Number(dispatches.to_string()),
            );
            object.insert(
                "readbacks".to_owned(),
                json::JsonValue::Number(readbacks.to_string()),
            );
        }
        runtime::RuntimeEvent::MlpStarted {
            model,
            train_path,
            test_path,
            checkpoint_path,
            train_sha256,
            test_sha256,
            checkpoint_before_sha256,
            train_samples,
            test_samples,
            steps,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            resumed_from,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.start".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "batch_size".to_owned(),
                json::JsonValue::Number(batch_size.to_string()),
            );
            object.insert(
                "hidden_size".to_owned(),
                json::JsonValue::Number(hidden_size.to_string()),
            );
            object.insert(
                "train_path".to_owned(),
                json::JsonValue::String(train_path.clone()),
            );
            object.insert(
                "test_path".to_owned(),
                json::JsonValue::String(test_path.clone()),
            );
            object.insert(
                "checkpoint_path".to_owned(),
                json::JsonValue::String(checkpoint_path.clone()),
            );
            object.insert(
                "train_sha256".to_owned(),
                json::JsonValue::String(train_sha256.clone()),
            );
            object.insert(
                "test_sha256".to_owned(),
                json::JsonValue::String(test_sha256.clone()),
            );
            object.insert(
                "checkpoint_before_sha256".to_owned(),
                checkpoint_before_sha256
                    .as_ref()
                    .map(|value| json::JsonValue::String(value.clone()))
                    .unwrap_or(json::JsonValue::Null),
            );
            object.insert(
                "train_samples".to_owned(),
                json::JsonValue::Number(train_samples.to_string()),
            );
            object.insert(
                "test_samples".to_owned(),
                json::JsonValue::Number(test_samples.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "resumed_from".to_owned(),
                json::JsonValue::Number(resumed_from.to_string()),
            );
            object.insert(
                "seed_semantics".to_owned(),
                json::JsonValue::String("deterministic_cpu_stream".to_owned()),
            );
        }
        runtime::RuntimeEvent::MlpStep {
            model,
            step,
            steps,
            updates,
            epoch,
            loss,
            evaluation_loss,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.step".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("step".to_owned(), json::JsonValue::Number(step.to_string()));
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert(
                "epoch".to_owned(),
                json::JsonValue::Number(epoch.to_string()),
            );
            object.insert(
                "loss".to_owned(),
                json::JsonValue::Number(format!("{loss:.8}")),
            );
            object.insert(
                "evaluation_loss".to_owned(),
                json::JsonValue::Number(format!("{evaluation_loss:.8}")),
            );
        }
        runtime::RuntimeEvent::MlpCompleted {
            model,
            steps,
            updates,
            seed,
            device,
            dtype,
            optimizer,
            learning_rate,
            batch_size,
            hidden_size,
            parameter_count,
            initial_loss,
            final_loss,
            evaluation_loss,
            loss_decreased,
            resumed_from,
            checkpoint_sha256,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.complete".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("train_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert("device".to_owned(), json::JsonValue::String(device.clone()));
            object.insert("dtype".to_owned(), json::JsonValue::String(dtype.clone()));
            object.insert(
                "optimizer".to_owned(),
                json::JsonValue::String(optimizer.clone()),
            );
            object.insert(
                "learning_rate".to_owned(),
                json::JsonValue::Number(format!("{learning_rate:.8}")),
            );
            object.insert(
                "batch_size".to_owned(),
                json::JsonValue::Number(batch_size.to_string()),
            );
            object.insert(
                "hidden_size".to_owned(),
                json::JsonValue::Number(hidden_size.to_string()),
            );
            object.insert(
                "parameter_count".to_owned(),
                json::JsonValue::Number(parameter_count.to_string()),
            );
            object.insert(
                "steps".to_owned(),
                json::JsonValue::Number(steps.to_string()),
            );
            object.insert(
                "updates".to_owned(),
                json::JsonValue::Number(updates.to_string()),
            );
            object.insert("seed".to_owned(), json::JsonValue::Number(seed.to_string()));
            object.insert(
                "initial_loss".to_owned(),
                json::JsonValue::Number(format!("{initial_loss:.8}")),
            );
            object.insert(
                "final_loss".to_owned(),
                json::JsonValue::Number(format!("{final_loss:.8}")),
            );
            object.insert(
                "evaluation_loss".to_owned(),
                json::JsonValue::Number(format!("{evaluation_loss:.8}")),
            );
            object.insert(
                "loss_decreased".to_owned(),
                json::JsonValue::Bool(*loss_decreased),
            );
            object.insert(
                "resumed_from".to_owned(),
                json::JsonValue::Number(resumed_from.to_string()),
            );
            object.insert(
                "checkpoint_sha256".to_owned(),
                json::JsonValue::String(checkpoint_sha256.clone()),
            );
        }
        runtime::RuntimeEvent::InferenceStarted {
            model,
            checkpoint_path,
            input_path,
            checkpoint_sha256,
            input_sha256,
            samples,
            input_size,
            output_size,
            checkpoint_step,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("inference.start".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("predict_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert(
                "device".to_owned(),
                json::JsonValue::String("cpu".to_owned()),
            );
            object.insert(
                "dtype".to_owned(),
                json::JsonValue::String("f32".to_owned()),
            );
            object.insert(
                "checkpoint_path".to_owned(),
                json::JsonValue::String(checkpoint_path.clone()),
            );
            object.insert(
                "input_path".to_owned(),
                json::JsonValue::String(input_path.clone()),
            );
            object.insert(
                "checkpoint_sha256".to_owned(),
                json::JsonValue::String(checkpoint_sha256.clone()),
            );
            object.insert(
                "input_sha256".to_owned(),
                json::JsonValue::String(input_sha256.clone()),
            );
            object.insert(
                "samples".to_owned(),
                json::JsonValue::Number(samples.to_string()),
            );
            object.insert(
                "input_size".to_owned(),
                json::JsonValue::Number(input_size.to_string()),
            );
            object.insert(
                "output_size".to_owned(),
                json::JsonValue::Number(output_size.to_string()),
            );
            object.insert(
                "checkpoint_step".to_owned(),
                json::JsonValue::Number(checkpoint_step.to_string()),
            );
        }
        runtime::RuntimeEvent::InferenceCompleted {
            model,
            samples,
            input_size,
            output_size,
            checkpoint_sha256,
            input_sha256,
            checkpoint_step,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("inference.complete".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String("predict_mlp".to_owned()),
            );
            object.insert("model".to_owned(), json::JsonValue::String(model.clone()));
            object.insert(
                "device".to_owned(),
                json::JsonValue::String("cpu".to_owned()),
            );
            object.insert(
                "dtype".to_owned(),
                json::JsonValue::String("f32".to_owned()),
            );
            object.insert(
                "samples".to_owned(),
                json::JsonValue::Number(samples.to_string()),
            );
            object.insert(
                "input_size".to_owned(),
                json::JsonValue::Number(input_size.to_string()),
            );
            object.insert(
                "output_size".to_owned(),
                json::JsonValue::Number(output_size.to_string()),
            );
            object.insert(
                "checkpoint_sha256".to_owned(),
                json::JsonValue::String(checkpoint_sha256.clone()),
            );
            object.insert(
                "input_sha256".to_owned(),
                json::JsonValue::String(input_sha256.clone()),
            );
            object.insert(
                "checkpoint_step".to_owned(),
                json::JsonValue::Number(checkpoint_step.to_string()),
            );
        }
        runtime::RuntimeEvent::Progress {
            operation,
            step,
            total,
            metric,
        } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("training.progress".to_owned()),
            );
            object.insert(
                "operation".to_owned(),
                json::JsonValue::String(operation.clone()),
            );
            object.insert("step".to_owned(), json::JsonValue::Number(step.to_string()));
            object.insert(
                "total".to_owned(),
                json::JsonValue::Number(total.to_string()),
            );
            object.insert(
                "metric".to_owned(),
                json::JsonValue::Number(format!("{metric:.8}")),
            );
        }
        runtime::RuntimeEvent::Audit { kind, payload } => {
            object.insert(
                "schema_version".to_owned(),
                json::JsonValue::Number("1".to_owned()),
            );
            object.insert(
                "event".to_owned(),
                json::JsonValue::String("audit.record".to_owned()),
            );
            object.insert(
                "audit_kind".to_owned(),
                json::JsonValue::String(kind.clone()),
            );
            let mut safe_payload = payload.clone();
            redact_sensitive_json(&mut safe_payload);
            object.insert("payload".to_owned(), safe_payload);
        }
    }
    json::encode(&json::JsonValue::Object(object))
}

pub(super) fn display_argument(argument: &OsStr) -> String {
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
