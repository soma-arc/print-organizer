use std::iter;
use std::time::Instant;
use eframe::{egui, wgpu};
use wgpu::util::DeviceExt;

use crate::graphics::{GlobalsUniform, create_render_pipeline, create_bind_group_layout};

pub struct MyApp {
    // wgpuリソース
    render_pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,
    
    // オフスクリーンテクスチャ
    offscreen_texture: wgpu::Texture,
    offscreen_view: wgpu::TextureView,
    
    // eguiテクスチャID
    texture_id: Option<egui::TextureId>,
    
    // レンダリングサイズ
    render_width: u32,
    render_height: u32,
    start_time: Instant,
}

impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("WGPU render state not available");
        
        let device = &render_state.device;
        
        let render_width = 800;
        let render_height = 600;
        
        // シェーダーの作成
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        
        // グローバルユニフォームの作成
        let globals = GlobalsUniform::new(render_width, render_height, 0.0);
        
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Uniform Buffer"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        let globals_bind_group_layout = create_bind_group_layout(device);
        
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals BindGroup"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        
        let render_pipeline = create_render_pipeline(device, &shader, &globals_bind_group_layout);
        
        // オフスクリーンテクスチャの作成
        let offscreen_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Texture"),
            size: wgpu::Extent3d {
                width: render_width,
                height: render_height,
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
        
        let offscreen_view = offscreen_texture.create_view(&wgpu::TextureViewDescriptor::default());
        
        // テクスチャをeguiに登録
        let texture_id = {
            let mut renderer = render_state.renderer.write();
            renderer.register_native_texture(
                device,
                &offscreen_view,
                wgpu::FilterMode::Linear,
            )
        };
        
        Self {
            render_pipeline,
            globals_buffer,
            globals_bind_group,
            offscreen_texture,
            offscreen_view,
            texture_id: Some(texture_id),
            render_width,
            render_height,
            start_time: Instant::now(),
        }
    }
    
    fn render_to_texture(&mut self, render_state: &eframe::egui_wgpu::RenderState) {
        let device = &render_state.device;
        let queue = &render_state.queue;

        let elapsed_time = self.start_time.elapsed().as_secs_f32();
        let globals = GlobalsUniform::new(self.render_width, self.render_height, elapsed_time);
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        
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

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // wgpuでオフスクリーンレンダリング
        if let Some(render_state) = frame.wgpu_render_state() {
            self.render_to_texture(render_state);
        }
        
        // eguiでUI構築
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Print Organizer");
            
            if let Some(texture_id) = self.texture_id {
                let size = egui::vec2(self.render_width as f32, self.render_height as f32);
                ui.image(egui::load::SizedTexture::new(texture_id, size));
            }
        });
        
        // 連続的に再描画
        ctx.request_repaint();
    }
}
