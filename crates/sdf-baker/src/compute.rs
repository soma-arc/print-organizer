use anyhow::{Context, Result};
use wgpu::util::DeviceExt;

use crate::gpu::GpuContext;
use crate::types::{BakeConfig, BrickResult, ComputeParams};

/// Bake a single brick by running the compute shader on the GPU.
///
/// Returns a `Vec<f32>` of length `brick_size^3` containing SDF values
/// in x-fastest order.
pub fn bake_brick(
    ctx: &GpuContext,
    pipeline: &wgpu::ComputePipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    config: &BakeConfig,
    bx: u32,
    by: u32,
    bz: u32,
) -> Result<Vec<f32>> {
    let b = config.brick_size;
    let num_voxels = (b * b * b) as usize;
    let output_size = (num_voxels * std::mem::size_of::<f32>()) as u64;

    // Uniform buffer with compute parameters
    let params = ComputeParams {
        aabb_min: config.aabb_min,
        voxel_size: config.voxel_size,
        brick_offset: [bx * b, by * b, bz * b],
        brick_size: b,
    };
    let uniform_buf = ctx
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params uniform"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });

    // Output storage buffer (GPU writes SDF values here)
    let output_buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("output storage"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    // Staging buffer for CPU readback
    let staging_buf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: output_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // Bind group
    let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("compute bind group"),
        layout: bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: output_buf.as_entire_binding(),
            },
        ],
    });

    // Dispatch compute
    let workgroup_size = 4u32;
    let dispatch = [
        (b + workgroup_size - 1) / workgroup_size,
        (b + workgroup_size - 1) / workgroup_size,
        (b + workgroup_size - 1) / workgroup_size,
    ];

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compute encoder"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("sdf compute"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, Some(&bind_group), &[]);
        pass.dispatch_workgroups(dispatch[0], dispatch[1], dispatch[2]);
    }

    // Copy output → staging
    encoder.copy_buffer_to_buffer(&output_buf, 0, &staging_buf, 0, output_size);

    ctx.queue.submit(Some(encoder.finish()));

    // Map staging buffer and read results
    let slice = staging_buf.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        sender.send(result).unwrap();
    });
    let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
    receiver
        .recv()
        .context("Channel closed")?
        .context("Buffer map failed")?;

    let data = slice.get_mapped_range();
    let values: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
    drop(data);
    staging_buf.unmap();

    Ok(values)
}

/// Create the compute pipeline and bind group layout from a WGSL shader module.
///
/// `entry_point` is typically `"cs_main"` for native WGSL or `"main"` for GLSL→WGSL.
pub fn create_compute_pipeline(
    device: &wgpu::Device,
    shader_source: &str,
    entry_point: &str,
) -> Result<(wgpu::ComputePipeline, wgpu::BindGroupLayout)> {
    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("sdf compute shader"),
        source: wgpu::ShaderSource::Wgsl(shader_source.into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("compute bind group layout"),
        entries: &[
            // @binding(0): Params uniform
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // @binding(1): output storage buffer
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("compute pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("sdf compute pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: Some(entry_point),
        compilation_options: Default::default(),
        cache: None,
    });

    Ok((pipeline, bind_group_layout))
}

/// Bake all bricks in the grid, with progress logging and sparse optimization.
///
/// Background-only bricks (all values at or beyond `background_value`) are marked
/// as inactive but still stored to preserve brick coordinates for the index.
pub fn bake_all_bricks(
    ctx: &GpuContext,
    pipeline: &wgpu::ComputePipeline,
    bind_group_layout: &wgpu::BindGroupLayout,
    config: &BakeConfig,
) -> Result<Vec<BrickResult>> {
    let counts = config.brick_counts();
    let total_bricks = (counts[0] * counts[1] * counts[2]) as usize;
    let mut results = Vec::with_capacity(total_bricks);
    let mut processed = 0usize;

    log::info!(
        "Baking {} bricks (grid {}x{}x{}, brick_size={})...",
        total_bricks,
        counts[0],
        counts[1],
        counts[2],
        config.brick_size,
    );

    for bz in 0..counts[2] {
        for by in 0..counts[1] {
            for bx in 0..counts[0] {
                processed += 1;
                if total_bricks > 1 {
                    log::info!("  Brick {processed}/{total_bricks} ({bx},{by},{bz})...");
                }

                let values = bake_brick(ctx, pipeline, bind_group_layout, config, bx, by, bz)?;

                // Sparse optimization: check if all values are at or beyond
                // the background distance (brick contains no surface)
                let is_background = values.iter().all(|&v| v.abs() >= config.background_value);

                if is_background {
                    log::debug!("  → background (skipped from output)");
                } else {
                    log::debug!("  → active ({} voxels)", values.len());
                }

                results.push(BrickResult {
                    bx,
                    by,
                    bz,
                    values,
                    is_background,
                });
            }
        }
    }

    let active = results.iter().filter(|b| !b.is_background).count();
    log::info!(
        "Baked {} bricks total, {} active, {} background",
        results.len(),
        active,
        results.len() - active,
    );

    Ok(results)
}
