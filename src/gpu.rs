//! Explicit GPU capability and compute validation.
//!
//! This module is deliberately narrower than a training backend. It proves
//! that Seloryn can select a physical non-fallback adapter, submit a WGSL compute
//! shader, synchronize completion and read results back with a bounded test.
//! Training remains CPU-only until tensor/device parity and checkpoint gates
//! are implemented.

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
