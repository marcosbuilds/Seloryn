//! Explicit GPU capability and compute validation.
//!
//! This module contains two bounded GPU gates: a physical compute/readback
//! probe and a deliberately small device-resident linear training profile.
//! The latter is not a general GPU tensor backend, but it does update weights
//! in a WGSL compute shader and performs one final readback.

use std::sync::mpsc;
use std::time::Duration;

const PROBE_VALUES: [f32; 8] = [1.0, -2.0, 3.5, 4.25, -5.5, 6.0, 7.75, 8.5];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterInfo {
    pub name: String,
    pub vendor: u32,
    pub device: u32,
    pub device_type: String,
    pub backend: String,
    pub driver: String,
    pub driver_info: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProbeReport {
    pub adapter: AdapterInfo,
    pub elements: usize,
    pub max_abs_error: f32,
    pub result: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinearTrainingReport {
    pub adapter: AdapterInfo,
    pub steps: usize,
    pub dispatches: usize,
    pub readbacks: usize,
    pub initial_loss: f32,
    pub final_loss: f32,
    pub weight: f32,
    pub bias: f32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Error {
    pub message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

pub fn enumerate_adapters() -> Vec<AdapterInfo> {
    let instance = instance();
    pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()))
        .into_iter()
        .map(|adapter| adapter_info(&adapter.get_info()))
        .collect()
}

pub fn run_probe() -> Result<ProbeReport, Error> {
    let instance = instance();
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    };
    let adapter = pollster::block_on(instance.request_adapter(&options))
        .map_err(|error| Error::new(format!("could not request a hardware adapter: {error:?}")))?;
    let info = adapter_info(&adapter.get_info());
    if !matches!(info.device_type.as_str(), "discretegpu" | "integratedgpu") {
        return Err(Error::new(format!(
            "adapter '{}' is not classified as a physical GPU (type={})",
            info.name, info.device_type
        )));
    }

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("seloryn-gpu-probe"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        experimental_features: Default::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| Error::new(format!("could not open GPU device: {error}")))?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("seloryn-gpu-probe-shader"),
        source: wgpu::ShaderSource::Wgsl(
            "struct Params { length: u32, }\n\n@group(0) @binding(0) var<storage, read> input_values: array<f32>;\n@group(0) @binding(1) var<storage, read_write> output_values: array<f32>;\n@group(0) @binding(2) var<uniform> params: Params;\n\n@compute @workgroup_size(8)\nfn main(@builtin(global_invocation_id) id: vec3<u32>) {\n    let index = id.x;\n    if index < params.length {\n        output_values[index] = input_values[index] * 2.0 + 1.0;\n    }\n}\n"
                .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("seloryn-gpu-probe-pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let input = storage_buffer(&device, &queue, "input", &PROBE_VALUES, false);
    let output = storage_buffer(&device, &queue, "output", &[0.0; PROBE_VALUES.len()], true);
    let params = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("params"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&params, 0, &(PROBE_VALUES.len() as u32).to_le_bytes());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (PROBE_VALUES.len() * std::mem::size_of::<f32>()) as u64,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("seloryn-gpu-probe-bind-group"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("seloryn-gpu-probe-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("seloryn-gpu-probe-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    encoder.copy_buffer_to_buffer(
        &output,
        0,
        &readback,
        0,
        (PROBE_VALUES.len() * std::mem::size_of::<f32>()) as u64,
    );
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|error| Error::new(format!("GPU submission did not complete: {error}")))?;
    receiver
        .recv()
        .map_err(|error| Error::new(format!("GPU readback callback failed: {error}")))?
        .map_err(|error| Error::new(format!("GPU readback failed: {error}")))?;
    let bytes = slice
        .get_mapped_range()
        .map_err(|error| Error::new(format!("GPU mapped readback failed: {error}")))?;
    let (chunks, remainder) = bytes.as_chunks::<4>();
    debug_assert!(remainder.is_empty());
    let result = chunks
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect::<Vec<_>>();
    drop(bytes);
    readback.unmap();
    let expected = PROBE_VALUES.map(|value| value * 2.0 + 1.0);
    let max_abs_error = result
        .iter()
        .zip(expected)
        .map(|(actual, expected)| (actual - expected).abs())
        .fold(0.0_f32, f32::max);
    if max_abs_error > 1.0e-6 {
        return Err(Error::new(format!(
            "GPU probe result exceeded tolerance: max_abs_error={max_abs_error}"
        )));
    }
    Ok(ProbeReport {
        adapter: info,
        elements: result.len(),
        max_abs_error,
        result,
    })
}

/// Train `y = weight * x + bias` with the parameters kept in a GPU storage
/// buffer for the complete optimization loop. This first GPU profile is
/// intentionally closed: scalar F32 data, at most 4096 samples and 1024
/// dispatches, no silent fallback and one final readback.
pub fn train_linear(
    train_x: &[f32],
    train_y: &[f32],
    steps: usize,
    learning_rate: f32,
) -> Result<LinearTrainingReport, Error> {
    const MAX_SAMPLES: usize = 4096;
    const MAX_STEPS: usize = 1024;
    if train_x.is_empty() || train_x.len() != train_y.len() {
        return Err(Error::new("GPU linear dataset x/y lengths are invalid"));
    }
    if train_x.len() > MAX_SAMPLES {
        return Err(Error::new(format!(
            "GPU linear dataset exceeds maximum {MAX_SAMPLES} samples"
        )));
    }
    if steps == 0 || steps > MAX_STEPS {
        return Err(Error::new(format!(
            "GPU linear steps must be between 1 and {MAX_STEPS}"
        )));
    }
    if !learning_rate.is_finite() || learning_rate <= 0.0 {
        return Err(Error::new(
            "GPU linear learning_rate must be positive and finite",
        ));
    }
    if train_x
        .iter()
        .chain(train_y)
        .any(|value| !value.is_finite())
    {
        return Err(Error::new(
            "GPU linear dataset contains a non-finite number",
        ));
    }
    let initial_loss =
        train_x.iter().zip(train_y).map(|(_, y)| y * y).sum::<f32>() / train_x.len() as f32;

    let instance = instance();
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
        apply_limit_buckets: false,
    };
    let adapter = pollster::block_on(instance.request_adapter(&options))
        .map_err(|error| Error::new(format!("GPU adapter request failed: {error:?}")))?;
    let info = adapter_info(&adapter.get_info());
    if !matches!(info.device_type.as_str(), "discretegpu" | "integratedgpu") {
        return Err(Error::new(format!(
            "GPU training requires a physical adapter, got type={} ({})",
            info.device_type, info.name
        )));
    }
    if adapter.limits().max_compute_workgroup_size_x < 1 {
        return Err(Error::new("GPU adapter cannot execute a compute workgroup"));
    }
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("seloryn-gpu-linear-training"),
        required_features: wgpu::Features::empty(),
        required_limits: wgpu::Limits::downlevel_defaults(),
        experimental_features: Default::default(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|error| Error::new(format!("GPU training device creation failed: {error}")))?;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("seloryn-gpu-linear-training-shader"),
        source: wgpu::ShaderSource::Wgsl(
            "struct Config { length: u32, learning_rate: f32, padding0: u32, padding1: u32, }\n\n@group(0) @binding(0) var<storage, read> input_values: array<f32>;\n@group(0) @binding(1) var<storage, read> target_values: array<f32>;\n@group(0) @binding(2) var<storage, read_write> parameters: array<f32>;\n@group(0) @binding(3) var<storage, read_write> metrics: array<f32>;\n@group(0) @binding(4) var<uniform> config: Config;\n\n@compute @workgroup_size(1)\nfn main(@builtin(global_invocation_id) id: vec3<u32>) {\n    if id.x != 0u { return; }\n    var weight = parameters[0];\n    var bias = parameters[1];\n    var weight_gradient = 0.0;\n    var bias_gradient = 0.0;\n    for (var index = 0u; index < 4096u; index = index + 1u) {\n        if index >= config.length { break; }\n        let error = weight * input_values[index] + bias - target_values[index];\n        weight_gradient = weight_gradient + 2.0 * error * input_values[index];\n        bias_gradient = bias_gradient + 2.0 * error;\n    }\n    let count = f32(config.length);\n    weight = weight - config.learning_rate * weight_gradient / count;\n    bias = bias - config.learning_rate * bias_gradient / count;\n    parameters[0] = weight;\n    parameters[1] = bias;\n    var loss = 0.0;\n    for (var index = 0u; index < 4096u; index = index + 1u) {\n        if index >= config.length { break; }\n        let error = weight * input_values[index] + bias - target_values[index];\n        loss = loss + error * error;\n    }\n    metrics[0] = loss / count;\n}\n"
                .into(),
        ),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("seloryn-gpu-linear-training-pipeline"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });

    let input = storage_buffer(&device, &queue, "linear-input", train_x, false);
    let target = storage_buffer(&device, &queue, "linear-target", train_y, false);
    let parameters = storage_buffer(&device, &queue, "linear-parameters", &[0.0, 0.0], true);
    let metrics = storage_buffer(&device, &queue, "linear-metrics", &[0.0], true);
    let config = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("linear-config"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut config_bytes = Vec::with_capacity(16);
    config_bytes.extend_from_slice(&(train_x.len() as u32).to_le_bytes());
    config_bytes.extend_from_slice(&learning_rate.to_le_bytes());
    config_bytes.extend_from_slice(&[0; 8]);
    queue.write_buffer(&config, 0, &config_bytes);
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("linear-readback"),
        size: 16,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("seloryn-gpu-linear-training-bind-group"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: target.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: parameters.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: metrics.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: config.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("seloryn-gpu-linear-training-encoder"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("seloryn-gpu-linear-training-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        for _ in 0..steps {
            pass.dispatch_workgroups(1, 1, 1);
        }
    }
    encoder.copy_buffer_to_buffer(&parameters, 0, &readback, 0, 8);
    encoder.copy_buffer_to_buffer(&metrics, 0, &readback, 8, 4);
    queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .map_err(|error| Error::new(format!("GPU training did not complete: {error}")))?;
    receiver
        .recv()
        .map_err(|error| Error::new(format!("GPU training readback callback failed: {error}")))?
        .map_err(|error| Error::new(format!("GPU training readback failed: {error}")))?;
    let bytes = slice
        .get_mapped_range()
        .map_err(|error| Error::new(format!("GPU training mapped readback failed: {error}")))?;
    let weight = f32::from_le_bytes(bytes[0..4].try_into().expect("weight bytes"));
    let bias = f32::from_le_bytes(bytes[4..8].try_into().expect("bias bytes"));
    let final_loss = f32::from_le_bytes(bytes[8..12].try_into().expect("loss bytes"));
    drop(bytes);
    readback.unmap();
    if !weight.is_finite() || !bias.is_finite() || !final_loss.is_finite() {
        return Err(Error::new("GPU training produced non-finite values"));
    }
    Ok(LinearTrainingReport {
        adapter: info,
        steps,
        dispatches: steps,
        readbacks: 1,
        initial_loss,
        final_loss,
        weight,
        bias,
    })
}

fn adapter_info(info: &wgpu::AdapterInfo) -> AdapterInfo {
    AdapterInfo {
        name: info.name.clone(),
        vendor: info.vendor,
        device: info.device,
        device_type: format!("{:?}", info.device_type).to_lowercase(),
        backend: format!("{:?}", info.backend).to_lowercase(),
        driver: info.driver.clone(),
        driver_info: info.driver_info.clone(),
    }
}

fn instance() -> wgpu::Instance {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    #[cfg(target_os = "windows")]
    {
        descriptor.backends = wgpu::Backends::VULKAN;
    }
    #[cfg(target_os = "macos")]
    {
        descriptor.backends = wgpu::Backends::METAL;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        descriptor.backends = wgpu::Backends::VULKAN;
    }
    wgpu::Instance::new(descriptor)
}

fn storage_buffer(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    values: &[f32],
    copy_src: bool,
) -> wgpu::Buffer {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(values));
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST
            | if copy_src {
                wgpu::BufferUsages::COPY_SRC
            } else {
                wgpu::BufferUsages::empty()
            },
        mapped_at_creation: false,
    });
    if !bytes.is_empty() {
        queue.write_buffer(&buffer, 0, &bytes);
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::enumerate_adapters;

    #[test]
    fn adapter_enumeration_is_bounded() {
        assert!(enumerate_adapters().len() <= 16);
    }
}
