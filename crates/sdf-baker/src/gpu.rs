use anyhow::{Context, Result};

/// Holds a wgpu device and queue obtained in headless (compute-only) mode.
pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_name: String,
}

/// Initialize a headless wgpu device suitable for compute shaders.
///
/// Prefers high-performance (discrete) GPU. No surface is created.
pub fn init_gpu() -> Result<GpuContext> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });

    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }))
    .context("No suitable GPU adapter found")?;

    let adapter_info = adapter.get_info();
    let adapter_name = format!("{} ({:?})", adapter_info.name, adapter_info.backend);
    log::info!("GPU adapter: {adapter_name}");

    let required_limits = wgpu::Limits {
        max_storage_buffer_binding_size: 128 * 1024 * 1024, // 128 MB
        max_buffer_size: 128 * 1024 * 1024,
        ..wgpu::Limits::downlevel_defaults()
    };

    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("sdf-baker compute device"),
        required_features: wgpu::Features::empty(),
        required_limits,
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
        experimental_features: Default::default(),
    }))
    .context("Failed to create wgpu device")?;

    Ok(GpuContext {
        device,
        queue,
        adapter_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_gpu() {
        let ctx = init_gpu().expect("GPU init should succeed");
        assert!(!ctx.adapter_name.is_empty());
        log::info!("Adapter: {}", ctx.adapter_name);
    }
}
