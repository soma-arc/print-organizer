use std::iter;

use eframe::{egui, wgpu};
use wgpu::util::DeviceExt;

use crate::graphics::{GlobalsUniform, create_bind_group_layout, create_render_pipeline};
use crate::preview_compose;

use super::MyApp;
use super::camera::OrbitCamera;

/// GPU resource bundle created during `MyApp::new()`.
pub(super) struct GpuResources {
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub render_pipeline: wgpu::RenderPipeline,
    pub globals_buffer: wgpu::Buffer,
    pub globals_bind_group: wgpu::BindGroup,
    pub offscreen_texture: wgpu::Texture,
    pub offscreen_view: wgpu::TextureView,
    pub texture_id: egui::TextureId,
}

/// Initialize all GPU resources for the preview renderer.
pub(super) fn init_gpu_resources(
    render_state: &eframe::egui_wgpu::RenderState,
    camera: &OrbitCamera,
    render_width: u32,
    render_height: u32,
) -> GpuResources {
    let device = &render_state.device;

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Fallback Shader"),
        source: wgpu::ShaderSource::Wgsl(fallback_wgsl().into()),
    });

    let globals = GlobalsUniform::new(
        camera.position(),
        camera.target,
        [0.0, 1.0, 0.0],
        [0.0; 3],
        [0.0; 3],
        render_width,
        render_height,
        0.0,
        0.0,
        false,
        false,
        true,
    );

    let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Globals Uniform Buffer"),
        contents: bytemuck::bytes_of(&globals),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let bind_group_layout = create_bind_group_layout(device);

    let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Globals BindGroup"),
        layout: &bind_group_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: globals_buffer.as_entire_binding(),
        }],
    });

    let render_pipeline = create_render_pipeline(device, &shader, &bind_group_layout);

    let (offscreen_texture, offscreen_view) =
        create_offscreen_texture(device, render_width, render_height);

    let texture_id = {
        let mut renderer = render_state.renderer.write();
        renderer.register_native_texture(device, &offscreen_view, wgpu::FilterMode::Linear)
    };

    GpuResources {
        bind_group_layout,
        render_pipeline,
        globals_buffer,
        globals_bind_group,
        offscreen_texture,
        offscreen_view,
        texture_id,
    }
}

/// Fallback shader: dark gray solid color (no SDF needed).
pub(super) fn fallback_wgsl() -> String {
    r#"
struct Globals {
    camera_pos: vec3<f32>, _pad0: f32,
    camera_target: vec3<f32>, _pad1: f32,
    camera_up: vec3<f32>, _pad2: f32,
    aabb_min: vec3<f32>, _pad3: f32,
    aabb_size: vec3<f32>, _pad4: f32,
    resolution: vec2<f32>, time: f32, brick_size: f32,
    show_aabb: u32, show_bricks: u32, clip_aabb: u32, _pad6: u32,
};
@group(0) @binding(0) var<uniform> globals: Globals;
struct VertexOutput { @builtin(position) clip_position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0,-1.0), vec2<f32>(1.0,-1.0), vec2<f32>(-1.0,1.0),
        vec2<f32>(-1.0,1.0), vec2<f32>(1.0,-1.0), vec2<f32>(1.0,1.0),
    );
    let p = positions[vi];
    var out: VertexOutput;
    out.clip_position = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}
@fragment fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let g = mix(0.18, 0.12, in.uv.y);
    return vec4<f32>(g, g, g, 1.0);
}
"#
    .to_string()
}

pub(super) fn create_offscreen_texture(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Offscreen Texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl MyApp {
    /// Recompile the preview pipeline from user SDF.
    /// On error, keep the previous pipeline and store the error.
    pub(super) fn rebuild_preview_pipeline(
        &mut self,
        device: &wgpu::Device,
        lang: sdf_baker::shader_compose::ShaderLang,
        user_sdf: &str,
    ) {
        match preview_compose::compose_preview(lang, user_sdf) {
            Ok(wgsl) => {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("Preview Shader"),
                    source: wgpu::ShaderSource::Wgsl(wgsl.into()),
                });
                self.render_pipeline =
                    create_render_pipeline(device, &shader, &self.bind_group_layout);
                self.preview_active = true;
                self.shader_errors.clear();
            }
            Err(e) => {
                self.set_shader_error(e);
            }
        }
    }

    pub(super) fn render_to_texture(&mut self, render_state: &eframe::egui_wgpu::RenderState) {
        let queue = &render_state.queue;

        let elapsed_time = self.start_time.elapsed().as_secs_f32();

        let (aabb_min, aabb_size, brick_size_world) = self
            .config_info
            .as_ref()
            .map(|info| {
                let bs = info.voxel_size * info.brick_size as f32;
                (info.aabb_min, info.aabb_size, bs)
            })
            .unwrap_or(([0.0; 3], [0.0; 3], 0.0));

        let globals = GlobalsUniform::new(
            self.camera.position(),
            self.camera.target,
            [0.0, 1.0, 0.0],
            aabb_min,
            aabb_size,
            self.render_width,
            self.render_height,
            elapsed_time,
            brick_size_world,
            self.show_aabb,
            self.show_bricks,
            self.clip_aabb,
        );
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        let device = &render_state.device;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Offscreen Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.offscreen_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.globals_bind_group, &[]);
            render_pass.draw(0..6, 0..1);
        }

        queue.submit(iter::once(encoder.finish()));
    }
}
