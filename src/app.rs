use std::iter;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use eframe::{egui, wgpu};
use wgpu::util::DeviceExt;

use crate::graphics::{GlobalsUniform, create_render_pipeline, create_bind_group_layout};

// ---------------------------------------------------------------------------
// Bake pipeline (runs in background thread)
// ---------------------------------------------------------------------------

/// Result sent back from the bake thread via channel.
#[derive(Debug)]
pub enum BakeResult {
    Success {
        out_dir: PathBuf,
        triangles: Option<u64>,
        vertices: Option<u64>,
        elapsed_ms: f64,
    },
    Error(String),
}

/// Status of the bake pipeline.
#[derive(Debug)]
enum BakeStatus {
    Idle,
    Running,
    Done(BakeResult),
}

/// Run the full sdf-baker pipeline on a background thread.
fn spawn_bake(
    config_path: PathBuf,
    out_dir: PathBuf,
    force: bool,
    tx: mpsc::Sender<BakeResult>,
) {
    std::thread::spawn(move || {
        let result = run_bake_pipeline(&config_path, &out_dir, force);
        let _ = tx.send(result);
    });
}

fn run_bake_pipeline(config_path: &PathBuf, out_dir: &PathBuf, force: bool) -> BakeResult {
    use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
    use sdf_baker::config::load_config;
    use sdf_baker::bricks_writer::{write_bricks, write_manifest};
    use sdf_baker::genmesh_runner::{run_genmesh, GenmeshRunConfig};
    use sdf_baker::gpu::init_gpu;
    use sdf_baker::shader_compose::{compose_shader, load_shader, ShaderLang, BUILTIN_SPHERE_SDF};
    use sdf_baker::types::BakeConfig;

    let start = Instant::now();

    // 1. Load config file
    let cfg = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => return BakeResult::Error(format!("Config load failed: {e:#}")),
    };

    let cfg_dir = config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    // 2. Resolve parameters from config (all fields optional, use defaults)
    let aabb_min = cfg.grid.aabb_min.unwrap_or([0.0, 0.0, 0.0]);
    let aabb_size = cfg.grid.aabb_size.unwrap_or([64.0, 64.0, 64.0]);
    let voxel_size = cfg.grid.voxel_size.unwrap_or(1.0);
    let brick_size = cfg.grid.brick_size.unwrap_or(64);
    let half_width = cfg.bake.half_width.unwrap_or(3);
    let iso = cfg.mesh.iso.unwrap_or(0.0);
    let adaptivity = cfg.mesh.adaptivity.unwrap_or(0.0);
    let dtype = cfg.bake.dtype.clone().unwrap_or_else(|| "f32".to_string());
    let write_vdb = cfg.genmesh.write_vdb.unwrap_or(false);
    let skip_genmesh = cfg.genmesh.skip.unwrap_or(false);
    let genmesh_path = cfg.genmesh.path.clone();

    let bake_config = BakeConfig::new(
        aabb_min, aabb_size, voxel_size, brick_size, half_width, iso, adaptivity, dtype,
    );

    // 3. Resolve shader path
    let (lang, user_sdf) = if let Some(ref shader_rel) = cfg.shader {
        let shader_path = cfg_dir.join(shader_rel);
        match load_shader(&shader_path) {
            Ok(pair) => pair,
            Err(e) => return BakeResult::Error(format!("Shader load failed: {e:#}")),
        }
    } else {
        (ShaderLang::Wgsl, BUILTIN_SPHERE_SDF.to_string())
    };

    // 4. Prepare output directory
    if out_dir.exists() && !force {
        return BakeResult::Error(format!(
            "Output directory already exists: {}. Enable 'Force overwrite'.",
            out_dir.display()
        ));
    }
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        return BakeResult::Error(format!("Failed to create output dir: {e}"));
    }

    // 5. GPU init
    let ctx = match init_gpu() {
        Ok(c) => c,
        Err(e) => return BakeResult::Error(format!("GPU init failed: {e:#}")),
    };

    // 6. Compose shader & create pipeline
    let composed = match compose_shader(lang, &user_sdf) {
        Ok(c) => c,
        Err(e) => return BakeResult::Error(format!("Shader compile failed: {e:#}")),
    };
    let (pipeline, layout) =
        match create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point) {
            Ok(pair) => pair,
            Err(e) => return BakeResult::Error(format!("Pipeline creation failed: {e:#}")),
        };

    // 7. Bake all bricks
    let bricks = match bake_all_bricks(&ctx, &pipeline, &layout, &bake_config) {
        Ok(b) => b,
        Err(e) => return BakeResult::Error(format!("Bake failed: {e:#}")),
    };

    // 8. Write output
    if let Err(e) = write_manifest(out_dir, &bake_config) {
        return BakeResult::Error(format!("Write manifest failed: {e:#}"));
    }
    if let Err(e) = write_bricks(out_dir, &bake_config, &bricks) {
        return BakeResult::Error(format!("Write bricks failed: {e:#}"));
    }

    // 9. Run genmesh
    let mut triangles = None;
    let mut vertices = None;

    if !skip_genmesh {
        let genmesh_exe = genmesh_path
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("genmesh"));

        let genmesh_config = GenmeshRunConfig {
            genmesh_path: genmesh_exe,
            out_dir: out_dir.clone(),
            iso,
            adaptivity,
            write_vdb,
        };

        match run_genmesh(&genmesh_config) {
            Ok(result) => {
                if let Some(ref report) = result.report {
                    triangles = Some(report.stats.triangle_count);
                    vertices = Some(report.stats.vertex_count);
                }
            }
            Err(e) => return BakeResult::Error(format!("genmesh failed: {e:#}")),
        }
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    BakeResult::Success {
        out_dir: out_dir.clone(),
        triangles,
        vertices,
        elapsed_ms,
    }
}

// ---------------------------------------------------------------------------
// ConfigInfo — parsed config summary for display
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct ConfigInfo {
    shader: String,
    out: String,
    aabb_min: [f32; 3],
    aabb_size: [f32; 3],
    voxel_size: f32,
    brick_size: u32,
    dims: [u32; 3],
    brick_counts: [u32; 3],
    total_voxels: u64,
    half_width: u32,
    iso: f32,
    adaptivity: f32,
    genmesh_path: String,
    write_vdb: bool,
}

impl ConfigInfo {
    fn from_config(cfg: &sdf_baker::config::ConfigFile, cfg_dir: &std::path::Path) -> Self {
        let aabb_min = cfg.grid.aabb_min.unwrap_or([0.0, 0.0, 0.0]);
        let aabb_size = cfg.grid.aabb_size.unwrap_or([64.0, 64.0, 64.0]);
        let voxel_size = cfg.grid.voxel_size.unwrap_or(1.0);
        let brick_size = cfg.grid.brick_size.unwrap_or(64);
        let half_width = cfg.bake.half_width.unwrap_or(3);

        let dims = [
            (aabb_size[0] / voxel_size).ceil() as u32,
            (aabb_size[1] / voxel_size).ceil() as u32,
            (aabb_size[2] / voxel_size).ceil() as u32,
        ];
        let brick_counts = [
            (dims[0] + brick_size - 1) / brick_size,
            (dims[1] + brick_size - 1) / brick_size,
            (dims[2] + brick_size - 1) / brick_size,
        ];
        let total_voxels = dims[0] as u64 * dims[1] as u64 * dims[2] as u64;

        let shader = cfg
            .shader
            .as_ref()
            .map(|s| cfg_dir.join(s).display().to_string())
            .unwrap_or_else(|| "(built-in sphere)".into());

        let out = cfg
            .out
            .clone()
            .unwrap_or_else(|| "(not set)".into());

        ConfigInfo {
            shader,
            out,
            aabb_min,
            aabb_size,
            voxel_size,
            brick_size,
            dims,
            brick_counts,
            total_voxels,
            half_width,
            iso: cfg.mesh.iso.unwrap_or(0.0),
            adaptivity: cfg.mesh.adaptivity.unwrap_or(0.0),
            genmesh_path: cfg
                .genmesh
                .path
                .clone()
                .unwrap_or_else(|| "genmesh".into()),
            write_vdb: cfg.genmesh.write_vdb.unwrap_or(false),
        }
    }
}

// ---------------------------------------------------------------------------
// MyApp
// ---------------------------------------------------------------------------

pub struct MyApp {
    // wgpu リソース
    render_pipeline: wgpu::RenderPipeline,
    globals_buffer: wgpu::Buffer,
    globals_bind_group: wgpu::BindGroup,

    // オフスクリーンテクスチャ
    offscreen_texture: wgpu::Texture,
    offscreen_view: wgpu::TextureView,

    // egui テクスチャ ID
    texture_id: Option<egui::TextureId>,

    // レンダリングサイズ
    render_width: u32,
    render_height: u32,
    start_time: Instant,

    // --- sdf-baker GUI state ---
    config_path: Option<PathBuf>,
    config_info: Option<ConfigInfo>,
    config_error: Option<String>,

    out_dir_override: String,
    force_overwrite: bool,

    bake_status: BakeStatus,
    bake_rx: Option<mpsc::Receiver<BakeResult>>,

    /// Channel for file dialog results (path selected by user)
    file_rx: Option<mpsc::Receiver<PathBuf>>,
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

            config_path: None,
            config_info: None,
            config_error: None,
            out_dir_override: String::new(),
            force_overwrite: true,
            bake_status: BakeStatus::Idle,
            bake_rx: None,
            file_rx: None,
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

    /// Load a config JSON and populate config_info / config_error.
    fn load_config_file(&mut self, path: PathBuf) {
        let cfg_dir = path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf();
        match sdf_baker::config::load_config(&path) {
            Ok(cfg) => {
                let info = ConfigInfo::from_config(&cfg, &cfg_dir);
                // Pre-fill output dir override from config if available
                if self.out_dir_override.is_empty() {
                    if let Some(ref out) = cfg.out {
                        self.out_dir_override = cfg_dir.join(out).display().to_string();
                    }
                }
                self.config_info = Some(info);
                self.config_error = None;
            }
            Err(e) => {
                self.config_info = None;
                self.config_error = Some(format!("{e:#}"));
            }
        }
        self.config_path = Some(path);
        self.bake_status = BakeStatus::Idle;
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // ---------------------------------------------------------------
        // Poll async channels
        // ---------------------------------------------------------------

        // File dialog result
        if let Some(rx) = &self.file_rx {
            if let Ok(path) = rx.try_recv() {
                self.load_config_file(path);
                self.file_rx = None;
            }
        }

        // Bake result
        if let Some(rx) = &self.bake_rx {
            if let Ok(result) = rx.try_recv() {
                self.bake_status = BakeStatus::Done(result);
                self.bake_rx = None;
            }
        }

        // ---------------------------------------------------------------
        // Side panel — config & bake controls
        // ---------------------------------------------------------------
        egui::SidePanel::left("bake_panel")
            .default_width(340.0)
            .show(ctx, |ui| {
                ui.heading("SDF Baker");
                ui.separator();

                // --- Open JSON button ---
                if ui.button("📂 JSON を開く…").clicked() {
                    let (tx, rx) = mpsc::channel();
                    self.file_rx = Some(rx);
                    std::thread::spawn(move || {
                        let file = rfd::FileDialog::new()
                            .add_filter("JSON config", &["json"])
                            .pick_file();
                        if let Some(path) = file {
                            let _ = tx.send(path);
                        }
                    });
                }

                if let Some(ref path) = self.config_path {
                    ui.label(format!("📄 {}", path.display()));
                }

                // --- Config error ---
                if let Some(ref err) = self.config_error {
                    ui.colored_label(egui::Color32::RED, format!("⚠ {err}"));
                }

                // --- Config info display ---
                if let Some(ref info) = self.config_info {
                    ui.separator();
                    ui.label("--- Grid ---");
                    egui::Grid::new("grid_info").show(ui, |ui| {
                        ui.label("Shader:");
                        ui.label(&info.shader);
                        ui.end_row();

                        ui.label("AABB min:");
                        ui.label(format!("{:?}", info.aabb_min));
                        ui.end_row();

                        ui.label("AABB size:");
                        ui.label(format!("{:?}", info.aabb_size));
                        ui.end_row();

                        ui.label("Voxel size:");
                        ui.label(format!("{}", info.voxel_size));
                        ui.end_row();

                        ui.label("Brick size:");
                        ui.label(format!("{}", info.brick_size));
                        ui.end_row();

                        ui.label("Dims:");
                        ui.label(format!("{:?}", info.dims));
                        ui.end_row();

                        ui.label("Bricks:");
                        ui.label(format!("{:?}", info.brick_counts));
                        ui.end_row();

                        ui.label("Total voxels:");
                        ui.label(format!("{}", info.total_voxels));
                        ui.end_row();
                    });

                    ui.separator();
                    ui.label("--- Mesh ---");
                    egui::Grid::new("mesh_info").show(ui, |ui| {
                        ui.label("Half width:");
                        ui.label(format!("{}", info.half_width));
                        ui.end_row();

                        ui.label("Iso:");
                        ui.label(format!("{}", info.iso));
                        ui.end_row();

                        ui.label("Adaptivity:");
                        ui.label(format!("{}", info.adaptivity));
                        ui.end_row();

                        ui.label("genmesh:");
                        ui.label(&info.genmesh_path);
                        ui.end_row();

                        ui.label("Write VDB:");
                        ui.label(format!("{}", info.write_vdb));
                        ui.end_row();
                    });

                    // --- Output dir override ---
                    ui.separator();
                    ui.label("出力先ディレクトリ:");
                    ui.text_edit_singleline(&mut self.out_dir_override);
                    ui.checkbox(&mut self.force_overwrite, "上書き許可 (force)");

                    // --- Bake button ---
                    ui.separator();
                    let can_bake = !matches!(self.bake_status, BakeStatus::Running)
                        && !self.out_dir_override.is_empty();

                    ui.add_enabled_ui(can_bake, |ui| {
                        if ui.button("🔨 Bake & Export").clicked() {
                            let config_path = self.config_path.clone().unwrap();
                            let out_dir = PathBuf::from(&self.out_dir_override);
                            let force = self.force_overwrite;

                            let (tx, rx) = mpsc::channel();
                            self.bake_rx = Some(rx);
                            self.bake_status = BakeStatus::Running;

                            spawn_bake(config_path, out_dir, force, tx);
                        }
                    });

                    // --- Status display ---
                    match &self.bake_status {
                        BakeStatus::Idle => {}
                        BakeStatus::Running => {
                            ui.spinner();
                            ui.label("Baking…");
                        }
                        BakeStatus::Done(BakeResult::Success {
                            out_dir,
                            triangles,
                            vertices,
                            elapsed_ms,
                        }) => {
                            ui.colored_label(
                                egui::Color32::GREEN,
                                format!("✅ 完了 ({:.0} ms)", elapsed_ms),
                            );
                            ui.label(format!("出力: {}", out_dir.display()));
                            if let (Some(t), Some(v)) = (triangles, vertices) {
                                ui.label(format!("Triangles: {t}, Vertices: {v}"));
                            }
                        }
                        BakeStatus::Done(BakeResult::Error(msg)) => {
                            ui.colored_label(egui::Color32::RED, format!("❌ {msg}"));
                        }
                    }
                }
            });

        // ---------------------------------------------------------------
        // Central panel — 3D preview
        // ---------------------------------------------------------------
        if let Some(render_state) = frame.wgpu_render_state() {
            self.render_to_texture(render_state);
        }

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
