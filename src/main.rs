use std::iter;
use eframe::{egui, wgpu};
use wgpu::util::DeviceExt;
use std::time::Instant;

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

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GlobalsUniform {
    resolution: [f32; 2],
    time: f32,
    _pad: [f32; 1],
}

impl MyApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("WGPU render state not available");
        
        let device = &render_state.device;
        let queue = &render_state.queue;
        
        let render_width = 800;
        let render_height = 600;
        
        // シェーダーの作成
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        
        // グローバルユニフォームの作成
        let globals = GlobalsUniform {
            resolution: [render_width as f32, render_height as f32],
            time: 0.0,
            _pad: [0.0],
        };
        
        let globals_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Globals Uniform Buffer"),
            contents: bytemuck::bytes_of(&globals),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        
        let globals_bind_group_layout = 
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Globals BindGroupLayout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });
        
        let globals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Globals BindGroup"),
            layout: &globals_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buffer.as_entire_binding(),
            }],
        });
        
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[&globals_bind_group_layout],
                push_constant_ranges: &[],
            });
        
        // レンダーパイプラインの作成（Rgba8Unorm形式）
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent::REPLACE,
                        alpha: wgpu::BlendComponent::REPLACE,
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });
        
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
        let globals = GlobalsUniform {
            resolution: [self.render_width as f32, self.render_height as f32],
            time: elapsed_time,
            _pad: [0.0],
        };
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

fn main() -> eframe::Result {
    env_logger::init();
    
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 680.0])
            .with_title("Print Organizer"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    
    eframe::run_native(
        "Print Organizer",
        options,
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}
