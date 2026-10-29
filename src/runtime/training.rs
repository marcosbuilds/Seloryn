use super::{
    EventSink, LINEAR_DEVICE, LINEAR_DTYPE, LINEAR_LEARNING_RATE, LINEAR_OPTIMIZER,
    LINEAR_PARAMETER_COUNT, MAX_CHECKPOINT_BYTES, MAX_MLP_WORK_UNITS, MAX_TRAINING_DATASET_BYTES,
    MAX_TRAINING_HIDDEN, MAX_TRAINING_INPUTS, MAX_TRAINING_OUTPUTS, MAX_TRAINING_PARAMETERS,
    MAX_TRAINING_SAMPLES, MAX_TRAINING_STEPS, MAX_TRAINING_UPDATES, MLP_CHECKPOINT_ARCHITECTURE,
    MLP_CHECKPOINT_LEGACY_SCHEMA, MLP_CHECKPOINT_RUNTIME, MLP_CHECKPOINT_SCHEMA,
    MLP_RNG_NONZERO_FALLBACK, MLP_RNG_SEED_MIX, RuntimeEvent,
};
use crate::compute::gpu;
use crate::data::sha256_hex;
use crate::json::{self, JsonValue};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::Path;

#[allow(clippy::too_many_arguments)]
pub(super) fn train_linear_json(
    train_file: &Path,
    test_file: &Path,
    train_path: &str,
    test_path: &str,
    seed: i64,
    steps: i64,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    let train = read_xy_dataset(train_file, train_path, json_max_bytes, json_max_depth)?;
    let test = read_xy_dataset(test_file, test_path, json_max_bytes, json_max_depth)?;
    if steps < 0 {
        return Err("training steps must be non-negative".to_owned());
    }
    if steps > MAX_TRAINING_STEPS {
        return Err(format!(
            "training steps exceed maximum {MAX_TRAINING_STEPS}"
        ));
    }
    let total_updates = train
        .0
        .len()
        .checked_mul(steps as usize)
        .ok_or_else(|| "training update count overflowed".to_owned())?;
    if total_updates > MAX_TRAINING_UPDATES {
        return Err(format!(
            "training updates exceed maximum {MAX_TRAINING_UPDATES}"
        ));
    }
    let mut weight = 0.0;
    let mut bias = 0.0;
    let initial_loss = mean_squared_error(&train, weight, bias);
    if !initial_loss.is_finite() {
        return Err("training produced a non-finite initial loss".to_owned());
    }
    event_sink.emit(RuntimeEvent::Started {
        model: "linear".to_owned(),
        train_path: train_path.to_owned(),
        test_path: test_path.to_owned(),
        train_sha256: train.2.clone(),
        test_sha256: test.2.clone(),
        train_samples: train.0.len(),
        test_samples: test.0.len(),
        steps,
        seed,
        device: LINEAR_DEVICE.to_owned(),
        dtype: LINEAR_DTYPE.to_owned(),
        optimizer: LINEAR_OPTIMIZER.to_owned(),
        learning_rate: LINEAR_LEARNING_RATE,
        parameter_count: LINEAR_PARAMETER_COUNT,
    })?;
    let progress_interval = (steps / 10).max(1);
    for step in 1..=steps {
        let count = train.0.len() as f64;
        let (weight_gradient, bias_gradient) = train.0.iter().zip(&train.1).fold(
            (0.0, 0.0),
            |(weight_gradient, bias_gradient), (x, y)| {
                let error = weight * x + bias - y;
                (
                    weight_gradient + (2.0 * error * x) / count,
                    bias_gradient + (2.0 * error) / count,
                )
            },
        );
        weight -= LINEAR_LEARNING_RATE * weight_gradient;
        bias -= LINEAR_LEARNING_RATE * bias_gradient;
        if step == 1 || step == steps || step % progress_interval == 0 {
            let loss = mean_squared_error(&train, weight, bias);
            if !loss.is_finite() {
                return Err(format!(
                    "training produced a non-finite loss at step {step}"
                ));
            }
            event_sink.emit(RuntimeEvent::Step {
                model: "linear".to_owned(),
                step,
                steps,
                updates: (step as usize) * train.0.len(),
                loss,
                loss_delta: initial_loss - loss,
            })?;
        }
    }
    let final_loss = mean_squared_error(&train, weight, bias);
    if !final_loss.is_finite() {
        return Err("training produced a non-finite final loss".to_owned());
    }
    let holdout_mae_observed = test
        .0
        .iter()
        .zip(&test.1)
        .map(|(x, y)| (weight * x + bias - y).abs())
        .fold(0.0, f64::max);
    if !holdout_mae_observed.is_finite() {
        return Err("training produced a non-finite holdout error".to_owned());
    }
    const HOLDOUT_MAE_MAX: f64 = 0.05;
    if holdout_mae_observed > HOLDOUT_MAE_MAX {
        return Err(format!(
            "holdout MAE {holdout_mae_observed:.8} exceeds maximum {HOLDOUT_MAE_MAX:.2}"
        ));
    }
    event_sink.emit(RuntimeEvent::Completed {
        model: "linear".to_owned(),
        steps,
        updates: total_updates,
        seed,
        device: LINEAR_DEVICE.to_owned(),
        dtype: LINEAR_DTYPE.to_owned(),
        optimizer: LINEAR_OPTIMIZER.to_owned(),
        learning_rate: LINEAR_LEARNING_RATE,
        parameter_count: LINEAR_PARAMETER_COUNT,
        initial_loss,
        final_loss,
        loss_delta: initial_loss - final_loss,
        loss_decreased: final_loss < initial_loss,
        holdout_mae_observed,
        weight,
        bias,
    })?;
    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "loss_decreased".to_owned(),
            JsonValue::Bool(final_loss < initial_loss),
        ),
        (
            "holdout_mae_max".to_owned(),
            JsonValue::Number(format!("{HOLDOUT_MAE_MAX:.2}")),
        ),
        (
            "holdout_mae_observed".to_owned(),
            JsonValue::Number(format!("{holdout_mae_observed:.8}")),
        ),
        ("seed".to_owned(), JsonValue::Number(seed.to_string())),
    ])))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn train_linear_gpu_json(
    train_file: &Path,
    test_file: &Path,
    train_path: &str,
    test_path: &str,
    seed: i64,
    steps: i64,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    const LEARNING_RATE: f32 = 0.02;
    let train = read_xy_dataset(train_file, train_path, json_max_bytes, json_max_depth)?;
    let test = read_xy_dataset(test_file, test_path, json_max_bytes, json_max_depth)?;
    let steps =
        usize::try_from(steps).map_err(|_| "GPU linear steps must be a positive I64".to_owned())?;
    let train_x = train
        .0
        .iter()
        .map(|value| *value as f32)
        .collect::<Vec<_>>();
    let train_y = train
        .1
        .iter()
        .map(|value| *value as f32)
        .collect::<Vec<_>>();
    event_sink.emit(RuntimeEvent::GpuStarted {
        model: "linear_gpu".to_owned(),
        train_path: train_path.to_owned(),
        test_path: test_path.to_owned(),
        train_sha256: train.2.clone(),
        test_sha256: test.2.clone(),
        train_samples: train_x.len(),
        steps,
        seed,
        device: "gpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate: LEARNING_RATE,
        parameter_count: 2,
        adapter: "selected_physical_adapter".to_owned(),
        backend: "wgpu".to_owned(),
    })?;
    let report = gpu::train_linear(&train_x, &train_y, steps, LEARNING_RATE)
        .map_err(|error| error.message)?;
    let holdout_mae_observed = test
        .0
        .iter()
        .zip(&test.1)
        .map(|(x, y)| (f64::from(report.weight) * x + f64::from(report.bias) - y).abs())
        .fold(0.0, f64::max) as f32;
    if !holdout_mae_observed.is_finite() {
        return Err("GPU training produced a non-finite holdout error".to_owned());
    }
    let loss_decreased = report.final_loss < report.initial_loss;
    event_sink.emit(RuntimeEvent::GpuCompleted {
        model: "linear_gpu".to_owned(),
        steps: report.steps,
        seed,
        device: "gpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate: LEARNING_RATE,
        parameter_count: 2,
        initial_loss: report.initial_loss,
        final_loss: report.final_loss,
        loss_decreased,
        holdout_mae_observed,
        weight: report.weight,
        bias: report.bias,
        adapter: report.adapter.name.clone(),
        backend: report.adapter.backend.clone(),
        dispatches: report.dispatches,
        readbacks: report.readbacks,
    })?;
    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "operation".to_owned(),
            JsonValue::String("train_linear_gpu".to_owned()),
        ),
        ("device".to_owned(), JsonValue::String("gpu".to_owned())),
        ("fallback".to_owned(), JsonValue::Bool(false)),
        ("dtype".to_owned(), JsonValue::String("f32".to_owned())),
        ("optimizer".to_owned(), JsonValue::String("sgd".to_owned())),
        (
            "steps".to_owned(),
            JsonValue::Number(report.steps.to_string()),
        ),
        (
            "dispatches".to_owned(),
            JsonValue::Number(report.dispatches.to_string()),
        ),
        (
            "readbacks".to_owned(),
            JsonValue::Number(report.readbacks.to_string()),
        ),
        (
            "initial_loss".to_owned(),
            JsonValue::Number(format_f32(report.initial_loss)),
        ),
        (
            "final_loss".to_owned(),
            JsonValue::Number(format_f32(report.final_loss)),
        ),
        ("loss_decreased".to_owned(), JsonValue::Bool(loss_decreased)),
        (
            "holdout_mae_observed".to_owned(),
            JsonValue::Number(format_f32(holdout_mae_observed)),
        ),
        (
            "weight".to_owned(),
            JsonValue::Number(format_f32(report.weight)),
        ),
        (
            "bias".to_owned(),
            JsonValue::Number(format_f32(report.bias)),
        ),
        ("adapter".to_owned(), JsonValue::String(report.adapter.name)),
        (
            "backend".to_owned(),
            JsonValue::String(report.adapter.backend),
        ),
        ("seed".to_owned(), JsonValue::Number(seed.to_string())),
    ])))
}

struct MlpDataset {
    inputs: Vec<Vec<f32>>,
    targets: Vec<Vec<f32>>,
    input_size: usize,
    output_size: usize,
    sha256: String,
}

struct MlpModel {
    input_size: usize,
    hidden_size: usize,
    output_size: usize,
    first_weights: Vec<f32>,
    first_bias: Vec<f32>,
    second_weights: Vec<f32>,
    second_bias: Vec<f32>,
}

struct MlpState {
    model: MlpModel,
    step: i64,
    cursor: usize,
    epoch: usize,
    order: Vec<usize>,
    rng_state: u64,
}

pub(super) struct MlpTrainingRequest<'a> {
    pub(super) train_file: &'a Path,
    pub(super) test_file: &'a Path,
    pub(super) checkpoint_file: &'a Path,
    pub(super) train_path: &'a str,
    pub(super) test_path: &'a str,
    pub(super) checkpoint_path: &'a str,
    pub(super) seed: i64,
    pub(super) steps: i64,
    pub(super) batch_size: i64,
    pub(super) hidden_size: i64,
    pub(super) learning_rate: f32,
}

pub(super) fn train_mlp_json(
    request: MlpTrainingRequest<'_>,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    let MlpTrainingRequest {
        train_file,
        test_file,
        checkpoint_file,
        train_path,
        test_path,
        checkpoint_path,
        seed,
        steps,
        batch_size,
        hidden_size,
        learning_rate,
    } = request;
    let train = read_mlp_dataset(train_file, train_path, json_max_bytes, json_max_depth)?;
    let test = read_mlp_dataset(test_file, test_path, json_max_bytes, json_max_depth)?;
    if train.input_size != test.input_size || train.output_size != test.output_size {
        return Err(
            "train and test datasets must have identical input/output dimensions".to_owned(),
        );
    }
    if !(0..=MAX_TRAINING_STEPS).contains(&steps) {
        return Err(format!(
            "training steps must be between 0 and {MAX_TRAINING_STEPS}"
        ));
    }
    let batch_size = usize::try_from(batch_size)
        .map_err(|_| "training batch_size must be positive".to_owned())?;
    if batch_size == 0 || batch_size > train.inputs.len() {
        return Err(format!(
            "training batch_size must be between 1 and {}",
            train.inputs.len()
        ));
    }
    let sample_updates = usize::try_from(steps)
        .ok()
        .and_then(|value| value.checked_mul(batch_size))
        .ok_or_else(|| "MLP sample update count overflowed".to_owned())?;
    if sample_updates > MAX_TRAINING_UPDATES {
        return Err(format!(
            "MLP sample updates exceed maximum {MAX_TRAINING_UPDATES}"
        ));
    }
    let hidden_size = usize::try_from(hidden_size)
        .map_err(|_| "training hidden_size must be positive".to_owned())?;
    if hidden_size == 0 || hidden_size > MAX_TRAINING_HIDDEN {
        return Err(format!(
            "training hidden_size must be between 1 and {MAX_TRAINING_HIDDEN}"
        ));
    }
    let parameter_count = train
        .input_size
        .checked_mul(hidden_size)
        .and_then(|value| value.checked_add(hidden_size))
        .and_then(|value| value.checked_add(hidden_size.checked_mul(train.output_size)?))
        .and_then(|value| value.checked_add(train.output_size))
        .ok_or_else(|| "MLP parameter count overflowed".to_owned())?;
    if parameter_count > MAX_TRAINING_PARAMETERS {
        return Err(format!(
            "MLP parameter count exceeds maximum {MAX_TRAINING_PARAMETERS}"
        ));
    }
    let work_units = sample_updates
        .checked_mul(parameter_count)
        .ok_or_else(|| "MLP work unit count overflowed".to_owned())?;
    if work_units > MAX_MLP_WORK_UNITS {
        return Err(format!(
            "MLP work units exceed maximum {MAX_MLP_WORK_UNITS}"
        ));
    }
    if !learning_rate.is_finite() || learning_rate <= 0.0 {
        return Err("training learning_rate must be positive and finite".to_owned());
    }

    let mut resumed_from = 0_i64;
    let mut checkpoint_before_sha256 = None;
    let mut state = if checkpoint_file.is_file() {
        let checkpoint_max_bytes = json_max_bytes.min(MAX_CHECKPOINT_BYTES);
        let checkpoint_max_depth = json_max_depth.min(32);
        let source = read_bounded_utf8(checkpoint_file, checkpoint_max_bytes, "MLP checkpoint")?;
        checkpoint_before_sha256 = Some(sha256_hex(source.as_bytes()));
        let value = json::parse(&source, checkpoint_max_bytes, checkpoint_max_depth)
            .map_err(|error| error.message)?;
        let loaded = parse_mlp_checkpoint(
            &value,
            &train,
            &test,
            seed,
            batch_size,
            hidden_size,
            learning_rate,
        )?;
        resumed_from = loaded.step;
        loaded
    } else {
        let mut rng_state = initial_mlp_rng_state(seed);
        let model = initialize_mlp(
            train.input_size,
            hidden_size,
            train.output_size,
            &mut rng_state,
        );
        let mut state = MlpState {
            model,
            step: 0,
            cursor: 0,
            epoch: 0,
            order: (0..train.inputs.len()).collect(),
            rng_state,
        };
        shuffle_order(&mut state.order, &mut state.rng_state);
        state
    };
    if state.step > steps {
        return Err(format!(
            "checkpoint step {} is ahead of requested target {steps}",
            state.step
        ));
    }
    let initial_loss = mlp_loss(&state.model, &train)?;
    event_sink.emit(RuntimeEvent::MlpStarted {
        model: "mlp".to_owned(),
        train_path: train_path.to_owned(),
        test_path: test_path.to_owned(),
        checkpoint_path: checkpoint_path.to_owned(),
        train_sha256: train.sha256.clone(),
        test_sha256: test.sha256.clone(),
        checkpoint_before_sha256,
        train_samples: train.inputs.len(),
        test_samples: test.inputs.len(),
        steps,
        seed,
        device: "cpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate,
        batch_size,
        hidden_size,
        parameter_count,
        resumed_from,
    })?;

    let progress_interval = (steps / 10).max(1);
    while state.step < steps {
        if state.cursor >= state.order.len() {
            state.cursor = 0;
            state.epoch = state.epoch.saturating_add(1);
            shuffle_order(&mut state.order, &mut state.rng_state);
        }
        let end = (state.cursor + batch_size).min(state.order.len());
        let batch = state.order[state.cursor..end].to_vec();
        state.cursor = end;
        mlp_update(&mut state.model, &train, &batch, learning_rate)?;
        state.step += 1;
        if state.step == 1 || state.step == steps || state.step % progress_interval == 0 {
            let loss = mlp_loss(&state.model, &train)?;
            let evaluation_loss = mlp_loss(&state.model, &test)?;
            event_sink.emit(RuntimeEvent::MlpStep {
                model: "mlp".to_owned(),
                step: state.step,
                steps,
                updates: state.step as usize,
                epoch: state.epoch,
                loss,
                evaluation_loss,
            })?;
            let checkpoint =
                encode_mlp_checkpoint(&state, &train, &test, seed, batch_size, learning_rate);
            write_checkpoint_atomic(checkpoint_file, &checkpoint)?;
        }
    }
    let final_loss = mlp_loss(&state.model, &train)?;
    let evaluation_loss = mlp_loss(&state.model, &test)?;
    let checkpoint = encode_mlp_checkpoint(&state, &train, &test, seed, batch_size, learning_rate);
    let checkpoint_sha256 = write_checkpoint_atomic(checkpoint_file, &checkpoint)?;
    let loss_decreased = final_loss < initial_loss;
    event_sink.emit(RuntimeEvent::MlpCompleted {
        model: "mlp".to_owned(),
        steps,
        updates: state.step as usize,
        seed,
        device: "cpu".to_owned(),
        dtype: "f32".to_owned(),
        optimizer: "sgd".to_owned(),
        learning_rate,
        batch_size,
        hidden_size,
        parameter_count,
        initial_loss,
        final_loss,
        evaluation_loss,
        loss_decreased,
        resumed_from,
        checkpoint_sha256: checkpoint_sha256.clone(),
    })?;
    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "operation".to_owned(),
            JsonValue::String("train_mlp".to_owned()),
        ),
        ("model".to_owned(), JsonValue::String("mlp".to_owned())),
        ("device".to_owned(), JsonValue::String("cpu".to_owned())),
        ("dtype".to_owned(), JsonValue::String("f32".to_owned())),
        ("optimizer".to_owned(), JsonValue::String("sgd".to_owned())),
        (
            "steps".to_owned(),
            JsonValue::Number(state.step.to_string()),
        ),
        (
            "batch_size".to_owned(),
            JsonValue::Number(batch_size.to_string()),
        ),
        (
            "hidden_size".to_owned(),
            JsonValue::Number(hidden_size.to_string()),
        ),
        (
            "parameter_count".to_owned(),
            JsonValue::Number(parameter_count.to_string()),
        ),
        (
            "initial_loss".to_owned(),
            JsonValue::Number(format_f32(initial_loss)),
        ),
        (
            "final_loss".to_owned(),
            JsonValue::Number(format_f32(final_loss)),
        ),
        (
            "evaluation_loss".to_owned(),
            JsonValue::Number(format_f32(evaluation_loss)),
        ),
        ("loss_decreased".to_owned(), JsonValue::Bool(loss_decreased)),
        (
            "resumed_from".to_owned(),
            JsonValue::Number(resumed_from.to_string()),
        ),
        (
            "checkpoint_sha256".to_owned(),
            JsonValue::String(checkpoint_sha256),
        ),
    ])))
}

fn read_mlp_dataset(
    path: &Path,
    label: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<MlpDataset, String> {
    let dataset_max_bytes = json_max_bytes.min(MAX_TRAINING_DATASET_BYTES);
    let dataset_max_depth = json_max_depth.min(32);
    let source = read_bounded_utf8(
        path,
        dataset_max_bytes,
        &format!("training dataset `{label}`"),
    )?;
    let sha256 = sha256_hex(source.as_bytes());
    let value = json::parse(&source, dataset_max_bytes, dataset_max_depth)
        .map_err(|error| error.message)?;
    let JsonValue::Object(values) = value else {
        return Err("MLP dataset must be a JSON object".to_owned());
    };
    let Some(JsonValue::Array(input_values)) = values.get("inputs") else {
        return Err("MLP dataset is missing inputs".to_owned());
    };
    let Some(JsonValue::Array(target_values)) = values.get("targets") else {
        return Err("MLP dataset is missing targets".to_owned());
    };
    if input_values.is_empty() || input_values.len() != target_values.len() {
        return Err("MLP dataset inputs/targets lengths are invalid".to_owned());
    }
    if input_values.len() > MAX_TRAINING_SAMPLES {
        return Err(format!(
            "MLP dataset exceeds maximum {MAX_TRAINING_SAMPLES} samples"
        ));
    }
    let inputs = input_values
        .iter()
        .map(|value| json_vector(value, "inputs"))
        .collect::<Result<Vec<_>, _>>()?;
    let targets = target_values
        .iter()
        .map(|value| json_vector(value, "targets"))
        .collect::<Result<Vec<_>, _>>()?;
    let input_size = inputs[0].len();
    let output_size = targets[0].len();
    if input_size == 0 || input_size > MAX_TRAINING_INPUTS {
        return Err(format!(
            "MLP input size must be between 1 and {MAX_TRAINING_INPUTS}"
        ));
    }
    if output_size == 0 || output_size > MAX_TRAINING_OUTPUTS {
        return Err(format!(
            "MLP output size must be between 1 and {MAX_TRAINING_OUTPUTS}"
        ));
    }
    if inputs.iter().any(|row| row.len() != input_size)
        || targets.iter().any(|row| row.len() != output_size)
    {
        return Err("MLP dataset rows must have uniform dimensions".to_owned());
    }
    Ok(MlpDataset {
        inputs,
        targets,
        input_size,
        output_size,
        sha256,
    })
}

struct MlpInputBatch {
    inputs: Vec<Vec<f32>>,
    input_size: usize,
    sha256: String,
}

pub(super) fn predict_mlp_json(
    checkpoint_file: &Path,
    input_file: &Path,
    checkpoint_path: &str,
    input_path: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
    event_sink: &mut dyn EventSink,
) -> Result<JsonValue, String> {
    let checkpoint_max_bytes = json_max_bytes.min(MAX_CHECKPOINT_BYTES);
    let checkpoint_source =
        read_bounded_utf8(checkpoint_file, checkpoint_max_bytes, "MLP checkpoint")?;
    let checkpoint_sha256 = sha256_hex(checkpoint_source.as_bytes());
    let checkpoint = json::parse(
        &checkpoint_source,
        checkpoint_max_bytes,
        json_max_depth.min(32),
    )
    .map_err(|error| error.message)?;
    let (model, checkpoint_step) = parse_mlp_model_checkpoint(&checkpoint)?;
    let inputs = read_mlp_inputs(input_file, input_path, json_max_bytes, json_max_depth)?;
    if inputs.input_size != model.input_size {
        return Err(format!(
            "MLP inference input dimension {} does not match checkpoint dimension {}",
            inputs.input_size, model.input_size
        ));
    }

    event_sink.emit(RuntimeEvent::InferenceStarted {
        model: "mlp".to_owned(),
        checkpoint_path: checkpoint_path.to_owned(),
        input_path: input_path.to_owned(),
        checkpoint_sha256: checkpoint_sha256.clone(),
        input_sha256: inputs.sha256.clone(),
        samples: inputs.inputs.len(),
        input_size: model.input_size,
        output_size: model.output_size,
        checkpoint_step,
    })?;

    let mut pre_activation = vec![0.0; model.hidden_size];
    let mut hidden = vec![0.0; model.hidden_size];
    let mut output = vec![0.0; model.output_size];
    let mut predictions = Vec::with_capacity(inputs.inputs.len());
    for input in &inputs.inputs {
        forward_mlp(&model, input, &mut pre_activation, &mut hidden, &mut output);
        predictions.push(json_f32_values(&output));
    }
    event_sink.emit(RuntimeEvent::InferenceCompleted {
        model: "mlp".to_owned(),
        samples: inputs.inputs.len(),
        input_size: model.input_size,
        output_size: model.output_size,
        checkpoint_sha256: checkpoint_sha256.clone(),
        input_sha256: inputs.sha256.clone(),
        checkpoint_step,
    })?;

    Ok(JsonValue::Object(std::collections::BTreeMap::from([
        (
            "operation".to_owned(),
            JsonValue::String("predict_mlp".to_owned()),
        ),
        ("model".to_owned(), JsonValue::String("mlp".to_owned())),
        ("device".to_owned(), JsonValue::String("cpu".to_owned())),
        ("dtype".to_owned(), JsonValue::String("f32".to_owned())),
        (
            "checkpoint_sha256".to_owned(),
            JsonValue::String(checkpoint_sha256),
        ),
        ("input_sha256".to_owned(), JsonValue::String(inputs.sha256)),
        (
            "checkpoint_step".to_owned(),
            JsonValue::Number(checkpoint_step.to_string()),
        ),
        (
            "samples".to_owned(),
            JsonValue::Number(inputs.inputs.len().to_string()),
        ),
        (
            "input_size".to_owned(),
            JsonValue::Number(model.input_size.to_string()),
        ),
        (
            "output_size".to_owned(),
            JsonValue::Number(model.output_size.to_string()),
        ),
        ("predictions".to_owned(), JsonValue::Array(predictions)),
    ])))
}

fn read_mlp_inputs(
    path: &Path,
    label: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<MlpInputBatch, String> {
    let input_max_bytes = json_max_bytes.min(MAX_TRAINING_DATASET_BYTES);
    let source = read_bounded_utf8(path, input_max_bytes, &format!("MLP input `{label}`"))?;
    let sha256 = sha256_hex(source.as_bytes());
    let value = json::parse(&source, input_max_bytes, json_max_depth.min(32))
        .map_err(|error| error.message)?;
    let JsonValue::Object(values) = value else {
        return Err("MLP inference input must be a JSON object".to_owned());
    };
    let Some(JsonValue::Array(input_values)) = values.get("inputs") else {
        return Err("MLP inference input is missing inputs".to_owned());
    };
    if input_values.is_empty() {
        return Err("MLP inference input must contain at least one row".to_owned());
    }
    if input_values.len() > MAX_TRAINING_SAMPLES {
        return Err(format!(
            "MLP inference input exceeds maximum {MAX_TRAINING_SAMPLES} samples"
        ));
    }
    let inputs = input_values
        .iter()
        .map(|value| json_vector(value, "inputs"))
        .collect::<Result<Vec<_>, _>>()?;
    let input_size = inputs[0].len();
    if input_size == 0 || input_size > MAX_TRAINING_INPUTS {
        return Err(format!(
            "MLP inference input size must be between 1 and {MAX_TRAINING_INPUTS}"
        ));
    }
    if inputs.iter().any(|row| row.len() != input_size) {
        return Err("MLP inference input rows must have uniform dimensions".to_owned());
    }
    Ok(MlpInputBatch {
        inputs,
        input_size,
        sha256,
    })
}

fn checkpoint_string<'a>(
    values: &'a BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a str, String> {
    match values.get(field) {
        Some(JsonValue::String(value)) => Ok(value),
        Some(_) => Err(format!("MLP checkpoint field `{field}` must be a string")),
        None => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn checkpoint_object<'a>(
    values: &'a BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a BTreeMap<String, JsonValue>, String> {
    match values.get(field) {
        Some(JsonValue::Object(value)) => Ok(value),
        Some(_) => Err(format!("MLP checkpoint field `{field}` must be an object")),
        None => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn reject_unknown_checkpoint_fields(
    values: &BTreeMap<String, JsonValue>,
    allowed: &[&str],
    scope: &str,
) -> Result<(), String> {
    if let Some(field) = values
        .keys()
        .find(|field| !allowed.iter().any(|allowed| allowed == field))
    {
        return Err(format!(
            "MLP checkpoint {scope} contains unsupported field `{field}`"
        ));
    }
    Ok(())
}

fn checkpoint_sha256<'a>(
    values: &'a BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<&'a str, String> {
    let value = checkpoint_string(values, field)?;
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "MLP checkpoint field `{field}` must be a lowercase SHA-256 digest"
        ));
    }
    Ok(value)
}

fn validate_checkpoint_shape(
    shapes: &BTreeMap<String, JsonValue>,
    field: &str,
    expected: &[usize],
) -> Result<(), String> {
    let actual = json_usize_array(shapes, field)?;
    if actual != expected {
        return Err(format!(
            "MLP checkpoint shape contract for `{field}` does not match the architecture"
        ));
    }
    Ok(())
}

fn validate_mlp_checkpoint_schema(values: &BTreeMap<String, JsonValue>) -> Result<(), String> {
    match values.get("schema") {
        Some(JsonValue::String(schema)) if schema == MLP_CHECKPOINT_SCHEMA => Ok(()),
        Some(JsonValue::String(schema)) if schema == MLP_CHECKPOINT_LEGACY_SCHEMA => Err(format!(
            "MLP checkpoint schema {MLP_CHECKPOINT_LEGACY_SCHEMA} is incompatible with this runtime; retrain to create {MLP_CHECKPOINT_SCHEMA}"
        )),
        _ => Err("MLP checkpoint schema is unsupported".to_owned()),
    }
}

fn validate_mlp_checkpoint_contract(
    values: &BTreeMap<String, JsonValue>,
    input_size: usize,
    hidden_size: usize,
    output_size: usize,
) -> Result<(), String> {
    validate_mlp_checkpoint_schema(values)?;
    reject_unknown_checkpoint_fields(
        values,
        &[
            "schema",
            "runtime",
            "model",
            "architecture",
            "device",
            "dtype",
            "step",
            "seed",
            "input_size",
            "hidden_size",
            "output_size",
            "batch_size",
            "learning_rate",
            "train_sha256",
            "test_sha256",
            "optimizer",
            "weights_sha256",
            "data_sha256",
            "state_sha256",
            "shapes",
            "features",
            "transform",
            "hash",
            "integrity",
            "weights",
            "data",
        ],
        "root",
    )?;
    if checkpoint_string(values, "runtime")? != MLP_CHECKPOINT_RUNTIME {
        return Err("MLP checkpoint runtime version is incompatible".to_owned());
    }
    if checkpoint_string(values, "model")? != "mlp" {
        return Err("MLP checkpoint model is unsupported".to_owned());
    }
    if checkpoint_string(values, "architecture")? != MLP_CHECKPOINT_ARCHITECTURE {
        return Err("MLP checkpoint architecture is unsupported".to_owned());
    }
    if checkpoint_string(values, "device")? != "cpu" {
        return Err("MLP checkpoint device is unsupported".to_owned());
    }
    if checkpoint_string(values, "dtype")? != "F32" {
        return Err("MLP checkpoint dtype is unsupported".to_owned());
    }

    let optimizer = checkpoint_object(values, "optimizer")?;
    reject_unknown_checkpoint_fields(optimizer, &["name", "state"], "optimizer")?;
    if checkpoint_string(optimizer, "name")? != "sgd"
        || checkpoint_string(optimizer, "state")? != "stateless"
    {
        return Err("MLP checkpoint optimizer is unsupported".to_owned());
    }

    let shapes = checkpoint_object(values, "shapes")?;
    reject_unknown_checkpoint_fields(
        shapes,
        &[
            "inputs",
            "first_weights",
            "first_bias",
            "second_weights",
            "second_bias",
            "outputs",
        ],
        "shapes",
    )?;
    validate_checkpoint_shape(shapes, "inputs", &[input_size])?;
    validate_checkpoint_shape(shapes, "first_weights", &[input_size, hidden_size])?;
    validate_checkpoint_shape(shapes, "first_bias", &[hidden_size])?;
    validate_checkpoint_shape(shapes, "second_weights", &[hidden_size, output_size])?;
    validate_checkpoint_shape(shapes, "second_bias", &[output_size])?;
    validate_checkpoint_shape(shapes, "outputs", &[output_size])?;

    let features = checkpoint_object(values, "features")?;
    reject_unknown_checkpoint_fields(features, &["kind", "order", "count"], "features")?;
    if checkpoint_string(features, "kind")? != "ordered_numeric_matrix"
        || checkpoint_string(features, "order")? != "source-column-order"
        || json_usize_field(features, "count")? != input_size
    {
        return Err("MLP checkpoint feature contract is incompatible".to_owned());
    }

    let transform = checkpoint_object(values, "transform")?;
    reject_unknown_checkpoint_fields(transform, &["name", "version"], "transform")?;
    if checkpoint_string(transform, "name")? != "identity"
        || json_u64_field(transform, "version")? != 1
    {
        return Err("MLP checkpoint transform contract is unsupported".to_owned());
    }

    for field in [
        "train_sha256",
        "test_sha256",
        "weights_sha256",
        "data_sha256",
        "state_sha256",
    ] {
        checkpoint_sha256(values, field)?;
    }
    let hashes = checkpoint_object(values, "hash")?;
    reject_unknown_checkpoint_fields(
        hashes,
        &["algorithm", "train", "test", "weights", "data", "state"],
        "hash",
    )?;
    if checkpoint_string(hashes, "algorithm")? != "sha256" {
        return Err("MLP checkpoint hash algorithm is unsupported".to_owned());
    }
    for (nested, top_level) in [
        ("train", "train_sha256"),
        ("test", "test_sha256"),
        ("weights", "weights_sha256"),
        ("data", "data_sha256"),
        ("state", "state_sha256"),
    ] {
        if checkpoint_sha256(hashes, nested)? != checkpoint_sha256(values, top_level)? {
            return Err(format!(
                "MLP checkpoint hash metadata for `{nested}` does not match the payload metadata"
            ));
        }
    }

    let integrity = checkpoint_object(values, "integrity")?;
    reject_unknown_checkpoint_fields(
        integrity,
        &["algorithm", "scope", "authenticity"],
        "integrity",
    )?;
    if checkpoint_string(integrity, "algorithm")? != "sha256"
        || checkpoint_string(integrity, "scope")? != "payload-sections"
        || checkpoint_string(integrity, "authenticity")? != "none"
    {
        return Err(
            "MLP checkpoint integrity contract is unsupported; SHA-256 does not authenticate authorship"
                .to_owned(),
        );
    }
    Ok(())
}

fn parse_mlp_model_checkpoint(value: &JsonValue) -> Result<(MlpModel, i64), String> {
    let JsonValue::Object(values) = value else {
        return Err("MLP checkpoint root must be an object".to_owned());
    };
    let step = json_i64_field(values, "step")?;
    if step < 0 {
        return Err("MLP checkpoint step cannot be negative".to_owned());
    }
    let input_size = json_usize_field(values, "input_size")?;
    let hidden_size = json_usize_field(values, "hidden_size")?;
    let output_size = json_usize_field(values, "output_size")?;
    validate_mlp_checkpoint_contract(values, input_size, hidden_size, output_size)?;
    if input_size == 0 || input_size > MAX_TRAINING_INPUTS {
        return Err(format!(
            "MLP checkpoint input size must be between 1 and {MAX_TRAINING_INPUTS}"
        ));
    }
    if hidden_size == 0 || hidden_size > MAX_TRAINING_HIDDEN {
        return Err(format!(
            "MLP checkpoint hidden size must be between 1 and {MAX_TRAINING_HIDDEN}"
        ));
    }
    if output_size == 0 || output_size > MAX_TRAINING_OUTPUTS {
        return Err(format!(
            "MLP checkpoint output size must be between 1 and {MAX_TRAINING_OUTPUTS}"
        ));
    }
    let parameter_count = input_size
        .checked_mul(hidden_size)
        .and_then(|value| value.checked_add(hidden_size))
        .and_then(|value| value.checked_add(hidden_size.checked_mul(output_size)?))
        .and_then(|value| value.checked_add(output_size))
        .ok_or_else(|| "MLP checkpoint parameter count overflowed".to_owned())?;
    if parameter_count > MAX_TRAINING_PARAMETERS {
        return Err(format!(
            "MLP checkpoint parameters exceed maximum {MAX_TRAINING_PARAMETERS}"
        ));
    }
    let model_values = match values.get("weights") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint weights are missing".to_owned()),
    };
    let actual_weights_hash =
        sha256_hex(json::encode(&JsonValue::Object(model_values.clone())).as_bytes());
    if !matches!(values.get("weights_sha256"), Some(JsonValue::String(value)) if value == &actual_weights_hash)
    {
        return Err("MLP checkpoint weights hash does not match the payload".to_owned());
    }
    let model = MlpModel {
        input_size,
        hidden_size,
        output_size,
        first_weights: json_f32_array(model_values, "first_weights")?,
        first_bias: json_f32_array(model_values, "first_bias")?,
        second_weights: json_f32_array(model_values, "second_weights")?,
        second_bias: json_f32_array(model_values, "second_bias")?,
    };
    if model.first_weights.len() != input_size * hidden_size
        || model.first_bias.len() != hidden_size
        || model.second_weights.len() != hidden_size * output_size
        || model.second_bias.len() != output_size
    {
        return Err("MLP checkpoint parameter shapes are invalid".to_owned());
    }
    let data = match values.get("data") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint data state is missing".to_owned()),
    };
    let actual_data_hash = sha256_hex(json::encode(&JsonValue::Object(data.clone())).as_bytes());
    if !matches!(values.get("data_sha256"), Some(JsonValue::String(value)) if value == &actual_data_hash)
    {
        return Err("MLP checkpoint data hash does not match the payload".to_owned());
    }
    let state_value = JsonValue::Object(std::collections::BTreeMap::from([
        ("step".to_owned(), JsonValue::Number(step.to_string())),
        (
            "weights".to_owned(),
            JsonValue::Object(model_values.clone()),
        ),
        ("data".to_owned(), JsonValue::Object(data.clone())),
    ]));
    let actual_state_hash = sha256_hex(json::encode(&state_value).as_bytes());
    if !matches!(values.get("state_sha256"), Some(JsonValue::String(value)) if value == &actual_state_hash)
    {
        return Err("MLP checkpoint training state hash does not match the payload".to_owned());
    }
    Ok((model, step))
}

fn json_vector(value: &JsonValue, label: &str) -> Result<Vec<f32>, String> {
    let JsonValue::Array(values) = value else {
        return Err(format!("MLP dataset {label} rows must be arrays"));
    };
    values
        .iter()
        .map(|value| {
            let JsonValue::Number(number) = value else {
                return Err(format!("MLP dataset {label} contains a non-number"));
            };
            let value = number
                .parse::<f32>()
                .map_err(|_| format!("MLP dataset {label} contains an invalid number"))?;
            value
                .is_finite()
                .then_some(value)
                .ok_or_else(|| format!("MLP dataset {label} contains a non-finite number"))
        })
        .collect()
}

fn initialize_mlp(
    input_size: usize,
    hidden_size: usize,
    output_size: usize,
    rng_state: &mut u64,
) -> MlpModel {
    let first_scale = (2.0 / input_size as f32).sqrt();
    let second_scale = (2.0 / hidden_size as f32).sqrt();
    let first_weights = (0..input_size * hidden_size)
        .map(|_| (next_unit(rng_state) * 2.0 - 1.0) * first_scale)
        .collect();
    let second_weights = (0..hidden_size * output_size)
        .map(|_| (next_unit(rng_state) * 2.0 - 1.0) * second_scale)
        .collect();
    MlpModel {
        input_size,
        hidden_size,
        output_size,
        first_weights,
        first_bias: vec![0.0; hidden_size],
        second_weights,
        second_bias: vec![0.0; output_size],
    }
}

fn mlp_update(
    model: &mut MlpModel,
    dataset: &MlpDataset,
    batch: &[usize],
    learning_rate: f32,
) -> Result<(), String> {
    let mut first_gradients = vec![0.0; model.first_weights.len()];
    let mut first_bias_gradients = vec![0.0; model.first_bias.len()];
    let mut second_gradients = vec![0.0; model.second_weights.len()];
    let mut second_bias_gradients = vec![0.0; model.second_bias.len()];
    let mut hidden = vec![0.0; model.hidden_size];
    let mut pre_activation = vec![0.0; model.hidden_size];
    let mut output = vec![0.0; model.output_size];
    let scale = 1.0 / (batch.len() * model.output_size) as f32;
    for &sample in batch {
        forward_mlp(
            model,
            &dataset.inputs[sample],
            &mut pre_activation,
            &mut hidden,
            &mut output,
        );
        let mut output_gradient = vec![0.0; model.output_size];
        for index in 0..model.output_size {
            output_gradient[index] = (output[index] - dataset.targets[sample][index]) * 2.0 * scale;
            second_bias_gradients[index] += output_gradient[index];
            for hidden_index in 0..model.hidden_size {
                second_gradients[hidden_index * model.output_size + index] +=
                    hidden[hidden_index] * output_gradient[index];
            }
        }
        for hidden_index in 0..model.hidden_size {
            let hidden_gradient = if pre_activation[hidden_index] > 0.0 {
                (0..model.output_size)
                    .map(|index| {
                        output_gradient[index]
                            * model.second_weights[hidden_index * model.output_size + index]
                    })
                    .sum::<f32>()
            } else {
                0.0
            };
            first_bias_gradients[hidden_index] += hidden_gradient;
            for input_index in 0..model.input_size {
                first_gradients[input_index * model.hidden_size + hidden_index] +=
                    dataset.inputs[sample][input_index] * hidden_gradient;
            }
        }
    }
    for (value, gradient) in model.first_weights.iter_mut().zip(first_gradients) {
        *value -= learning_rate * gradient;
    }
    for (value, gradient) in model.first_bias.iter_mut().zip(first_bias_gradients) {
        *value -= learning_rate * gradient;
    }
    for (value, gradient) in model.second_weights.iter_mut().zip(second_gradients) {
        *value -= learning_rate * gradient;
    }
    for (value, gradient) in model.second_bias.iter_mut().zip(second_bias_gradients) {
        *value -= learning_rate * gradient;
    }
    if model
        .first_weights
        .iter()
        .chain(&model.first_bias)
        .chain(&model.second_weights)
        .chain(&model.second_bias)
        .any(|value| !value.is_finite())
    {
        return Err("MLP update produced a non-finite parameter".to_owned());
    }
    Ok(())
}

fn forward_mlp(
    model: &MlpModel,
    input: &[f32],
    pre_activation: &mut [f32],
    hidden: &mut [f32],
    output: &mut [f32],
) {
    for hidden_index in 0..model.hidden_size {
        let value = model.first_bias[hidden_index]
            + input
                .iter()
                .enumerate()
                .map(|(input_index, value)| {
                    value * model.first_weights[input_index * model.hidden_size + hidden_index]
                })
                .sum::<f32>();
        pre_activation[hidden_index] = value;
        hidden[hidden_index] = value.max(0.0);
    }
    for (output_index, output_value) in output.iter_mut().enumerate().take(model.output_size) {
        *output_value = model.second_bias[output_index]
            + (0..model.hidden_size)
                .map(|hidden_index| {
                    hidden[hidden_index]
                        * model.second_weights[hidden_index * model.output_size + output_index]
                })
                .sum::<f32>();
    }
}

fn mlp_loss(model: &MlpModel, dataset: &MlpDataset) -> Result<f32, String> {
    let mut pre_activation = vec![0.0; model.hidden_size];
    let mut hidden = vec![0.0; model.hidden_size];
    let mut output = vec![0.0; model.output_size];
    let mut loss = 0.0;
    for (input, target) in dataset.inputs.iter().zip(&dataset.targets) {
        forward_mlp(model, input, &mut pre_activation, &mut hidden, &mut output);
        loss += output
            .iter()
            .zip(target)
            .map(|(predicted, target)| (predicted - target).powi(2))
            .sum::<f32>();
    }
    let loss = loss / (dataset.inputs.len() * model.output_size) as f32;
    loss.is_finite()
        .then_some(loss)
        .ok_or_else(|| "MLP loss is non-finite".to_owned())
}

fn parse_mlp_checkpoint(
    value: &JsonValue,
    train: &MlpDataset,
    test: &MlpDataset,
    seed: i64,
    batch_size: usize,
    hidden_size: usize,
    learning_rate: f32,
) -> Result<MlpState, String> {
    let JsonValue::Object(values) = value else {
        return Err("MLP checkpoint root must be an object".to_owned());
    };
    validate_mlp_checkpoint_schema(values)?;
    if !matches!(values.get("train_sha256"), Some(JsonValue::String(value)) if value == &train.sha256)
        || !matches!(values.get("test_sha256"), Some(JsonValue::String(value)) if value == &test.sha256)
    {
        return Err("MLP checkpoint dataset hash does not match the current data".to_owned());
    }
    let checkpoint_seed = json_i64_field(values, "seed")?;
    if checkpoint_seed != seed {
        return Err("MLP checkpoint seed does not match the requested seed".to_owned());
    }
    let step = json_i64_field(values, "step")?;
    let checkpoint_batch = json_usize_field(values, "batch_size")?;
    let checkpoint_hidden = json_usize_field(values, "hidden_size")?;
    let checkpoint_learning_rate = json_f32_field(values, "learning_rate")?;
    let input_size = json_usize_field(values, "input_size")?;
    let output_size = json_usize_field(values, "output_size")?;
    validate_mlp_checkpoint_contract(values, input_size, checkpoint_hidden, output_size)?;
    if checkpoint_batch != batch_size
        || checkpoint_hidden != hidden_size
        || (checkpoint_learning_rate - learning_rate).abs() > f32::EPSILON
    {
        return Err("MLP checkpoint hyperparameters do not match the requested run".to_owned());
    }
    if step < 0 {
        return Err("MLP checkpoint step cannot be negative".to_owned());
    }
    if input_size != train.input_size || output_size != train.output_size {
        return Err("MLP checkpoint dimensions do not match the current data".to_owned());
    }
    let model_values = match values.get("weights") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint weights are missing".to_owned()),
    };
    let actual_weights_hash =
        sha256_hex(json::encode(&JsonValue::Object(model_values.clone())).as_bytes());
    if !matches!(values.get("weights_sha256"), Some(JsonValue::String(value)) if value == &actual_weights_hash)
    {
        return Err("MLP checkpoint weights hash does not match the payload".to_owned());
    }
    let model = MlpModel {
        input_size,
        hidden_size,
        output_size,
        first_weights: json_f32_array(model_values, "first_weights")?,
        first_bias: json_f32_array(model_values, "first_bias")?,
        second_weights: json_f32_array(model_values, "second_weights")?,
        second_bias: json_f32_array(model_values, "second_bias")?,
    };
    if model.first_weights.len() != input_size * hidden_size
        || model.first_bias.len() != hidden_size
        || model.second_weights.len() != hidden_size * output_size
        || model.second_bias.len() != output_size
    {
        return Err("MLP checkpoint parameter shapes are invalid".to_owned());
    }
    let data = match values.get("data") {
        Some(JsonValue::Object(value)) => value,
        _ => return Err("MLP checkpoint data state is missing".to_owned()),
    };
    let state_value = JsonValue::Object(std::collections::BTreeMap::from([
        ("step".to_owned(), JsonValue::Number(step.to_string())),
        (
            "weights".to_owned(),
            JsonValue::Object(model_values.clone()),
        ),
        ("data".to_owned(), JsonValue::Object(data.clone())),
    ]));
    let actual_state_hash = sha256_hex(json::encode(&state_value).as_bytes());
    if !matches!(values.get("state_sha256"), Some(JsonValue::String(value)) if value == &actual_state_hash)
    {
        return Err("MLP checkpoint training state hash does not match the payload".to_owned());
    }
    let actual_data_hash = sha256_hex(json::encode(&JsonValue::Object(data.clone())).as_bytes());
    if !matches!(values.get("data_sha256"), Some(JsonValue::String(value)) if value == &actual_data_hash)
    {
        return Err("MLP checkpoint data hash does not match the payload".to_owned());
    }
    let order = json_usize_array(data, "order")?;
    if order.len() != train.inputs.len()
        || order.iter().any(|index| *index >= train.inputs.len())
        || {
            let mut sorted = order.clone();
            sorted.sort_unstable();
            sorted != (0..train.inputs.len()).collect::<Vec<_>>()
        }
    {
        return Err("MLP checkpoint sampler order is invalid".to_owned());
    }
    let cursor = json_usize_field(data, "cursor")?;
    if cursor > order.len() {
        return Err("MLP checkpoint sampler cursor is invalid".to_owned());
    }
    Ok(MlpState {
        model,
        step,
        cursor,
        epoch: json_usize_field(data, "epoch")?,
        order,
        rng_state: json_u64_field(data, "rng_state")?,
    })
}

fn encode_mlp_checkpoint(
    state: &MlpState,
    train: &MlpDataset,
    test: &MlpDataset,
    seed: i64,
    batch_size: usize,
    learning_rate: f32,
) -> String {
    let model = &state.model;
    let weights = std::collections::BTreeMap::from([
        (
            "first_weights".to_owned(),
            json_f32_values(&model.first_weights),
        ),
        ("first_bias".to_owned(), json_f32_values(&model.first_bias)),
        (
            "second_weights".to_owned(),
            json_f32_values(&model.second_weights),
        ),
        (
            "second_bias".to_owned(),
            json_f32_values(&model.second_bias),
        ),
    ]);
    let data = std::collections::BTreeMap::from([
        (
            "cursor".to_owned(),
            JsonValue::Number(state.cursor.to_string()),
        ),
        (
            "epoch".to_owned(),
            JsonValue::Number(state.epoch.to_string()),
        ),
        (
            "order".to_owned(),
            JsonValue::Array(
                state
                    .order
                    .iter()
                    .map(|value| JsonValue::Number(value.to_string()))
                    .collect(),
            ),
        ),
        (
            "rng_state".to_owned(),
            JsonValue::Number(state.rng_state.to_string()),
        ),
    ]);
    let weights_value = JsonValue::Object(weights);
    let data_value = JsonValue::Object(data);
    let weights_sha256 = sha256_hex(json::encode(&weights_value).as_bytes());
    let data_sha256 = sha256_hex(json::encode(&data_value).as_bytes());
    let state_value = JsonValue::Object(std::collections::BTreeMap::from([
        ("step".to_owned(), JsonValue::Number(state.step.to_string())),
        ("weights".to_owned(), weights_value.clone()),
        ("data".to_owned(), data_value.clone()),
    ]));
    let state_sha256 = sha256_hex(json::encode(&state_value).as_bytes());
    let shapes_value = JsonValue::Object(BTreeMap::from([
        (
            "inputs".to_owned(),
            JsonValue::Array(vec![JsonValue::Number(train.input_size.to_string())]),
        ),
        (
            "first_weights".to_owned(),
            JsonValue::Array(vec![
                JsonValue::Number(train.input_size.to_string()),
                JsonValue::Number(model.hidden_size.to_string()),
            ]),
        ),
        (
            "first_bias".to_owned(),
            JsonValue::Array(vec![JsonValue::Number(model.hidden_size.to_string())]),
        ),
        (
            "second_weights".to_owned(),
            JsonValue::Array(vec![
                JsonValue::Number(model.hidden_size.to_string()),
                JsonValue::Number(train.output_size.to_string()),
            ]),
        ),
        (
            "second_bias".to_owned(),
            JsonValue::Array(vec![JsonValue::Number(train.output_size.to_string())]),
        ),
        (
            "outputs".to_owned(),
            JsonValue::Array(vec![JsonValue::Number(train.output_size.to_string())]),
        ),
    ]));
    let features_value = JsonValue::Object(BTreeMap::from([
        (
            "kind".to_owned(),
            JsonValue::String("ordered_numeric_matrix".to_owned()),
        ),
        (
            "order".to_owned(),
            JsonValue::String("source-column-order".to_owned()),
        ),
        (
            "count".to_owned(),
            JsonValue::Number(train.input_size.to_string()),
        ),
    ]));
    let transform_value = JsonValue::Object(BTreeMap::from([
        ("name".to_owned(), JsonValue::String("identity".to_owned())),
        ("version".to_owned(), JsonValue::Number("1".to_owned())),
    ]));
    let hash_value = JsonValue::Object(BTreeMap::from([
        (
            "algorithm".to_owned(),
            JsonValue::String("sha256".to_owned()),
        ),
        ("train".to_owned(), JsonValue::String(train.sha256.clone())),
        ("test".to_owned(), JsonValue::String(test.sha256.clone())),
        (
            "weights".to_owned(),
            JsonValue::String(weights_sha256.clone()),
        ),
        ("data".to_owned(), JsonValue::String(data_sha256.clone())),
        ("state".to_owned(), JsonValue::String(state_sha256.clone())),
    ]));
    let integrity_value = JsonValue::Object(BTreeMap::from([
        (
            "algorithm".to_owned(),
            JsonValue::String("sha256".to_owned()),
        ),
        (
            "scope".to_owned(),
            JsonValue::String("payload-sections".to_owned()),
        ),
        (
            "authenticity".to_owned(),
            JsonValue::String("none".to_owned()),
        ),
    ]));
    let checkpoint = JsonValue::Object(std::collections::BTreeMap::from([
        (
            "schema".to_owned(),
            JsonValue::String(MLP_CHECKPOINT_SCHEMA.to_owned()),
        ),
        (
            "runtime".to_owned(),
            JsonValue::String(MLP_CHECKPOINT_RUNTIME.to_owned()),
        ),
        ("model".to_owned(), JsonValue::String("mlp".to_owned())),
        (
            "architecture".to_owned(),
            JsonValue::String(MLP_CHECKPOINT_ARCHITECTURE.to_owned()),
        ),
        ("device".to_owned(), JsonValue::String("cpu".to_owned())),
        ("dtype".to_owned(), JsonValue::String("F32".to_owned())),
        ("step".to_owned(), JsonValue::Number(state.step.to_string())),
        ("seed".to_owned(), JsonValue::Number(seed.to_string())),
        (
            "input_size".to_owned(),
            JsonValue::Number(train.input_size.to_string()),
        ),
        (
            "hidden_size".to_owned(),
            JsonValue::Number(model.hidden_size.to_string()),
        ),
        (
            "output_size".to_owned(),
            JsonValue::Number(train.output_size.to_string()),
        ),
        (
            "batch_size".to_owned(),
            JsonValue::Number(batch_size.to_string()),
        ),
        (
            "learning_rate".to_owned(),
            JsonValue::Number(format_f32(learning_rate)),
        ),
        (
            "train_sha256".to_owned(),
            JsonValue::String(train.sha256.clone()),
        ),
        (
            "test_sha256".to_owned(),
            JsonValue::String(test.sha256.clone()),
        ),
        (
            "optimizer".to_owned(),
            JsonValue::Object(std::collections::BTreeMap::from([
                ("name".to_owned(), JsonValue::String("sgd".to_owned())),
                (
                    "state".to_owned(),
                    JsonValue::String("stateless".to_owned()),
                ),
            ])),
        ),
        (
            "weights_sha256".to_owned(),
            JsonValue::String(weights_sha256),
        ),
        ("data_sha256".to_owned(), JsonValue::String(data_sha256)),
        ("state_sha256".to_owned(), JsonValue::String(state_sha256)),
        ("shapes".to_owned(), shapes_value),
        ("features".to_owned(), features_value),
        ("transform".to_owned(), transform_value),
        ("hash".to_owned(), hash_value),
        ("integrity".to_owned(), integrity_value),
        ("weights".to_owned(), weights_value),
        ("data".to_owned(), data_value),
    ]));
    json::encode(&checkpoint)
}

fn write_checkpoint_atomic(path: &Path, source: &str) -> Result<String, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "checkpoint path has no parent directory".to_owned())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "checkpoint path has no valid file name".to_owned())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "checkpoint temporary file clock is invalid".to_owned())?
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("checkpoint temporary file creation failed: {error}"))?;
    let write_result = file
        .write_all(source.as_bytes())
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    if let Err(error) = write_result {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("checkpoint temporary file flush failed: {error}"));
    }
    drop(file);
    if let Err(error) = replace_checkpoint_atomically(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(sha256_hex(source.as_bytes()))
}

/// Persist a program-produced JSON value without exposing arbitrary file I/O.
///
/// The interpreter has already resolved `path` against the manifest's write
/// capability before this helper runs. The same-file temporary/flush/rename
/// discipline used by native checkpoints is retained for source-owned model
/// artifacts.
pub(super) fn write_json_atomic(path: &Path, source: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "JSON output path has no parent directory".to_owned())?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "JSON output path has no valid file name".to_owned())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "JSON output temporary file clock is invalid".to_owned())?
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("JSON output temporary file creation failed: {error}"))?;
    let write_result = file
        .write_all(source.as_bytes())
        .and_then(|()| file.flush())
        .and_then(|()| file.sync_all());
    if let Err(error) = write_result {
        drop(file);
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("JSON output temporary file flush failed: {error}"));
    }
    drop(file);
    if let Err(error) = replace_checkpoint_atomically(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(format!("JSON output atomic replacement failed: {error}"));
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_checkpoint_atomically(temporary: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(temporary, destination)
        .map_err(|error| format!("checkpoint atomic rename failed: {error}"))
}

#[cfg(windows)]
fn replace_checkpoint_atomically(temporary: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MoveFileExW(
            temporary.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(format!(
            "checkpoint atomic replacement failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn json_f32_values(values: &[f32]) -> JsonValue {
    JsonValue::Array(
        values
            .iter()
            .map(|value| JsonValue::Number(format_f32(*value)))
            .collect(),
    )
}

fn json_f32_array(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<Vec<f32>, String> {
    let Some(JsonValue::Array(values)) = values.get(field) else {
        return Err(format!("MLP checkpoint field `{field}` is missing"));
    };
    values
        .iter()
        .map(|value| match value {
            JsonValue::Number(value) => {
                let parsed = value.parse::<f32>().map_err(|_| {
                    format!("MLP checkpoint field `{field}` contains an invalid number")
                })?;
                parsed.is_finite().then_some(parsed).ok_or_else(|| {
                    format!("MLP checkpoint field `{field}` contains a non-finite number")
                })
            }
            _ => Err(format!(
                "MLP checkpoint field `{field}` contains a non-number"
            )),
        })
        .collect()
}

fn json_usize_array(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<Vec<usize>, String> {
    let Some(JsonValue::Array(values)) = values.get(field) else {
        return Err(format!("MLP checkpoint field `{field}` is missing"));
    };
    values
        .iter()
        .map(|value| match value {
            JsonValue::Number(value) => value
                .parse::<usize>()
                .map_err(|_| format!("MLP checkpoint field `{field}` contains an invalid integer")),
            _ => Err(format!(
                "MLP checkpoint field `{field}` contains a non-integer"
            )),
        })
        .collect()
}

fn json_i64_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<i64, String> {
    match values.get(field) {
        Some(JsonValue::Number(value)) => value
            .parse()
            .map_err(|_| format!("MLP checkpoint field `{field}` is not an integer")),
        _ => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn json_u64_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<u64, String> {
    match values.get(field) {
        Some(JsonValue::Number(value)) => value
            .parse::<u64>()
            .map_err(|_| format!("MLP checkpoint field `{field}` is not an unsigned integer")),
        _ => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn json_usize_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<usize, String> {
    json_u64_field(values, field).and_then(|value| {
        usize::try_from(value).map_err(|_| format!("MLP checkpoint field `{field}` is too large"))
    })
}

fn json_f32_field(
    values: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
) -> Result<f32, String> {
    match values.get(field) {
        Some(JsonValue::Number(value)) => {
            let parsed = value
                .parse::<f32>()
                .map_err(|_| format!("MLP checkpoint field `{field}` is not a finite number"))?;
            parsed
                .is_finite()
                .then_some(parsed)
                .ok_or_else(|| format!("MLP checkpoint field `{field}` is not a finite number"))
        }
        _ => Err(format!("MLP checkpoint field `{field}` is missing")),
    }
}

fn shuffle_order(order: &mut [usize], state: &mut u64) {
    for index in (1..order.len()).rev() {
        let swap = (next_u64(state) as usize) % (index + 1);
        order.swap(index, swap);
    }
}

pub(super) fn initial_mlp_rng_state(seed: i64) -> u64 {
    let state = (seed as u64) ^ MLP_RNG_SEED_MIX;
    if state == 0 {
        MLP_RNG_NONZERO_FALLBACK
    } else {
        state
    }
}

pub(super) fn next_u64(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *state = value;
    value
}

fn next_unit(state: &mut u64) -> f32 {
    (next_u64(state) as u32) as f32 / u32::MAX as f32
}

fn format_f32(value: f32) -> String {
    value.to_string()
}

fn read_xy_dataset(
    path: &Path,
    label: &str,
    json_max_bytes: usize,
    json_max_depth: usize,
) -> Result<(Vec<f64>, Vec<f64>, String), String> {
    let dataset_max_bytes = json_max_bytes.min(MAX_TRAINING_DATASET_BYTES);
    let dataset_max_depth = json_max_depth.min(16);
    let source = read_bounded_utf8(
        path,
        dataset_max_bytes,
        &format!("training dataset `{label}`"),
    )?;
    let value = json::parse(&source, dataset_max_bytes, dataset_max_depth)
        .map_err(|error| error.message)?;
    let JsonValue::Object(values) = value else {
        return Err("dataset must be a JSON object".to_owned());
    };
    let Some(JsonValue::Array(x_values)) = values.get("x") else {
        return Err("dataset is missing x".to_owned());
    };
    let Some(JsonValue::Array(y_values)) = values.get("y") else {
        return Err("dataset is missing y".to_owned());
    };
    let x = x_values
        .iter()
        .map(json_number)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "dataset x contains a non-number".to_owned())?;
    let y = y_values
        .iter()
        .map(json_number)
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "dataset y contains a non-number".to_owned())?;
    if x.iter().chain(&y).any(|value| !value.is_finite()) {
        return Err("dataset contains a non-finite number".to_owned());
    }
    if x.len() > MAX_TRAINING_SAMPLES || y.len() > MAX_TRAINING_SAMPLES {
        return Err(format!(
            "dataset exceeds maximum {MAX_TRAINING_SAMPLES} samples"
        ));
    }
    if x.is_empty() || x.len() != y.len() {
        return Err("dataset x/y lengths are invalid".to_owned());
    }
    Ok((x, y, sha256_hex(source.as_bytes())))
}

pub(super) fn read_bounded_utf8(
    path: &Path,
    max_bytes: usize,
    label: &str,
) -> Result<String, String> {
    let file =
        std::fs::File::open(path).map_err(|error| format!("{label} read failed: {error}"))?;
    if file
        .metadata()
        .map_err(|error| format!("{label} metadata read failed: {error}"))?
        .len()
        > max_bytes as u64
    {
        return Err(format!("{label} exceeds maximum {max_bytes} bytes"));
    }
    let mut bytes = Vec::new();
    file.take(max_bytes.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{label} read failed: {error}"))?;
    if bytes.len() > max_bytes {
        return Err(format!("{label} exceeds maximum {max_bytes} bytes"));
    }
    String::from_utf8(bytes).map_err(|_| format!("{label} is not valid UTF-8"))
}

fn json_number(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Number(value) => value.parse().ok(),
        _ => None,
    }
}

fn mean_squared_error(dataset: &(Vec<f64>, Vec<f64>, String), weight: f64, bias: f64) -> f64 {
    dataset
        .0
        .iter()
        .zip(&dataset.1)
        .map(|(x, y)| (weight * x + bias - y).powi(2))
        .sum::<f64>()
        / dataset.0.len() as f64
}

pub(super) fn validate_checkpoint(
    source: &str,
    max_bytes: usize,
    max_depth: usize,
) -> Result<JsonValue, String> {
    let value = json::parse(source, max_bytes, max_depth).map_err(|error| error.message)?;
    let JsonValue::Object(ref values) = value else {
        return Err("checkpoint root must be an object".to_owned());
    };
    let valid_schema = matches!(
        values.get("schema"),
        Some(JsonValue::String(schema)) if schema == "seloryn.checkpoint.v1"
    );
    let valid_step =
        matches!(values.get("step"), Some(JsonValue::Number(step)) if step.parse::<i64>().is_ok());
    let valid_weights = matches!(values.get("weights"), Some(JsonValue::Object(_)));
    let valid_optimizer = matches!(values.get("optimizer"), Some(JsonValue::Object(_)));
    let valid_fixture = matches!(
        values.get("fixture"),
        Some(JsonValue::String(fixture)) if fixture == "schema-valid"
    );
    if valid_schema && valid_step && valid_weights && valid_optimizer && valid_fixture {
        Ok(value)
    } else {
        Err("checkpoint failed schema or type validation".to_owned())
    }
}

pub(super) fn json_string(value: &str) -> String {
    json::encode(&JsonValue::String(value.to_owned()))
}

pub(super) fn format_f64(value: f64) -> String {
    let rendered = value.to_string();
    if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
        rendered
    } else {
        format!("{rendered}.0")
    }
}

fn escape_text(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{{{:x}}}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped
}

pub(super) fn render_text(value: &str, remaining: &mut usize) -> String {
    let length = value.chars().count();
    let visible_length = length.min(*remaining);
    let visible = value.chars().take(visible_length).collect::<String>();
    *remaining -= visible_length;
    let suffix = if visible_length < length {
        format!("... ({} more chars)", length - visible_length)
    } else {
        String::new()
    };
    format!("\"{}{}\"", escape_text(&visible), suffix)
}
