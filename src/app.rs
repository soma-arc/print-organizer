use std::iter;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use eframe::{egui, wgpu};
use wgpu::util::DeviceExt;

use crate::graphics::{GlobalsUniform, create_bind_group_layout, create_render_pipeline};
use crate::preview_compose;

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
fn spawn_bake(config_path: PathBuf, out_dir: PathBuf, force: bool, tx: mpsc::Sender<BakeResult>) {
    std::thread::spawn(move || {
        let result = run_bake_pipeline(&config_path, &out_dir, force);
        let _ = tx.send(result);
    });
}

fn run_bake_pipeline(config_path: &PathBuf, out_dir: &PathBuf, force: bool) -> BakeResult {
    use sdf_baker::bricks_writer::{write_bricks, write_manifest};
    use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
    use sdf_baker::config::load_config;
    use sdf_baker::genmesh_runner::{GenmeshRunConfig, run_genmesh};
    use sdf_baker::gpu::init_gpu;
    use sdf_baker::shader_compose::{BUILTIN_SPHERE_SDF, ShaderLang, compose_shader, load_shader};
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

        let out = cfg.out.clone().unwrap_or_else(|| "(not set)".into());

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
            genmesh_path: cfg.genmesh.path.clone().unwrap_or_else(|| "genmesh".into()),
            write_vdb: cfg.genmesh.write_vdb.unwrap_or(false),
        }
    }
}

// ---------------------------------------------------------------------------
// Camera (orbit)
// ---------------------------------------------------------------------------

struct OrbitCamera {
    target: [f32; 3],
    yaw: f32,   // radians
    pitch: f32, // radians
    distance: f32,
}

impl OrbitCamera {
    fn from_aabb(aabb_min: [f32; 3], aabb_size: [f32; 3]) -> Self {
        let cx = aabb_min[0] + aabb_size[0] * 0.5;
        let cy = aabb_min[1] + aabb_size[1] * 0.5;
        let cz = aabb_min[2] + aabb_size[2] * 0.5;
        let diag = (aabb_size[0] * aabb_size[0]
            + aabb_size[1] * aabb_size[1]
            + aabb_size[2] * aabb_size[2])
            .sqrt();
        Self {
            target: [cx, cy, cz],
            yaw: std::f32::consts::FRAC_PI_4, // 45°
            pitch: 0.5236,                    // 30°
            distance: diag * 1.5,
        }
    }

    fn position(&self) -> [f32; 3] {
        let cos_p = self.pitch.cos();
        let sin_p = self.pitch.sin();
        let cos_y = self.yaw.cos();
        let sin_y = self.yaw.sin();
        [
            self.target[0] + self.distance * cos_p * sin_y,
            self.target[1] + self.distance * sin_p,
            self.target[2] + self.distance * cos_p * cos_y,
        ]
    }
}

// ---------------------------------------------------------------------------
// MyApp
// ---------------------------------------------------------------------------

pub struct MyApp {
    // wgpu リソース（bind group layout は再利用するため保持）
    bind_group_layout: wgpu::BindGroupLayout,
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

    // --- preview state ---
    camera: OrbitCamera,
    /// true when a valid preview shader has been compiled
    preview_active: bool,
    /// Error from the last shader compilation attempt
    shader_error: Option<String>,
    /// true while camera/input is being manipulated (continuous repaint)
    needs_repaint: bool,
    /// Show AABB wireframe overlay
    show_aabb: bool,
    /// Show brick boundary wireframe overlay
    show_bricks: bool,
    /// Clip SDF rendering to AABB bounds
    clip_aabb: bool,

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

        // デフォルトフォールバックシェーダ（static circle, 動的プレビュー前の初期状態用）
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Fallback Shader"),
            source: wgpu::ShaderSource::Wgsl(Self::fallback_wgsl().into()),
        });

        // グローバルユニフォームの作成（初期カメラは原点向き）
        let camera = OrbitCamera {
            target: [0.0, 0.0, 0.0],
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.5236,
            distance: 100.0,
        };
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

        // オフスクリーンテクスチャの作成
        let (offscreen_texture, offscreen_view) =
            Self::create_offscreen_texture(device, render_width, render_height);

        // テクスチャをeguiに登録
        let texture_id = {
            let mut renderer = render_state.renderer.write();
            renderer.register_native_texture(device, &offscreen_view, wgpu::FilterMode::Linear)
        };

        Self {
            bind_group_layout,
            render_pipeline,
            globals_buffer,
            globals_bind_group,
            offscreen_texture,
            offscreen_view,
            texture_id: Some(texture_id),
            render_width,
            render_height,
            start_time: Instant::now(),

            camera,
            preview_active: false,
            shader_error: None,
            needs_repaint: false,
            show_aabb: true,
            show_bricks: false,
            clip_aabb: true,

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

    /// Fallback shader: dark gray solid color (no SDF needed).
    fn fallback_wgsl() -> String {
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

    fn create_offscreen_texture(
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

    /// Recompile the preview pipeline from user SDF.
    /// On error, keep the previous pipeline and store the error.
    fn rebuild_preview_pipeline(
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
                self.shader_error = None;
            }
            Err(e) => {
                self.shader_error = Some(format!("{e:#}"));
                // Keep previous pipeline as fallback
            }
        }
    }

    fn render_to_texture(&mut self, render_state: &eframe::egui_wgpu::RenderState) {
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

    /// Load a config JSON and populate config_info / config_error.
    /// Also rebuilds the preview pipeline if a shader is specified.
    fn load_config_file(&mut self, path: PathBuf, device: &wgpu::Device) {
        let cfg_dir = path
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .to_path_buf();
        match sdf_baker::config::load_config(&path) {
            Ok(cfg) => {
                let info = ConfigInfo::from_config(&cfg, &cfg_dir);
                // Always update output dir from the new config
                self.out_dir_override = cfg
                    .out
                    .as_ref()
                    .map(|out| cfg_dir.join(out).display().to_string())
                    .unwrap_or_default();

                // Rebuild camera to fit new AABB
                self.camera = OrbitCamera::from_aabb(info.aabb_min, info.aabb_size);

                // Rebuild preview pipeline from shader
                if let Some(ref shader_rel) = cfg.shader {
                    let shader_path = cfg_dir.join(shader_rel);
                    match sdf_baker::shader_compose::load_shader(&shader_path) {
                        Ok((lang, user_sdf)) => {
                            self.rebuild_preview_pipeline(device, lang, &user_sdf);
                        }
                        Err(e) => {
                            self.shader_error = Some(format!("{e:#}"));
                            self.preview_active = false;
                        }
                    }
                } else {
                    // No shader specified — use built-in sphere
                    self.rebuild_preview_pipeline(
                        device,
                        sdf_baker::shader_compose::ShaderLang::Wgsl,
                        sdf_baker::shader_compose::BUILTIN_SPHERE_SDF,
                    );
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
        self.needs_repaint = true;
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.needs_repaint = false;

        // ---------------------------------------------------------------
        // Grab device reference for pipeline operations
        // ---------------------------------------------------------------
        let device: Option<wgpu::Device> = frame.wgpu_render_state().map(|rs| rs.device.clone());

        // ---------------------------------------------------------------
        // Poll async channels
        // ---------------------------------------------------------------

        // File dialog result
        if let Some(rx) = &self.file_rx {
            if let Ok(path) = rx.try_recv() {
                if let Some(ref device) = device {
                    self.load_config_file(path, device);
                }
                self.file_rx = None;
            }
        }

        // Bake result
        if let Some(rx) = &self.bake_rx {
            if let Ok(result) = rx.try_recv() {
                self.bake_status = BakeStatus::Done(result);
                self.bake_rx = None;
                self.needs_repaint = true;
            }
        }

        // Drag & drop JSON files
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .filter(|p| p.extension().is_some_and(|e| e == "json"))
                .collect()
        });
        if let Some(path) = dropped.into_iter().next() {
            if let Some(ref device) = device {
                self.load_config_file(path, device);
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

                    // --- Preview overlays ---
                    ui.separator();
                    ui.label("--- Preview ---");
                    if ui.checkbox(&mut self.show_aabb, "AABB 表示").changed() {
                        self.needs_repaint = true;
                    }
                    if ui
                        .checkbox(&mut self.show_bricks, "ブリック境界 表示")
                        .changed()
                    {
                        self.needs_repaint = true;
                    }
                    if ui
                        .checkbox(&mut self.clip_aabb, "AABB クリップ")
                        .changed()
                    {
                        self.needs_repaint = true;
                    }

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
        egui::CentralPanel::default().show(ctx, |ui| {
            // --- shader error banner ---
            if let Some(ref err) = self.shader_error {
                ui.colored_label(egui::Color32::YELLOW, format!("⚠ Shader: {err}"));
            }

            let available = ui.available_size();
            let new_w = (available.x as u32).clamp(64, 4096);
            let new_h = (available.y as u32).clamp(64, 4096);

            // L6: resize offscreen texture when panel size changes
            if new_w != self.render_width || new_h != self.render_height {
                self.render_width = new_w;
                self.render_height = new_h;

                if let Some(render_state) = frame.wgpu_render_state() {
                    let device = &render_state.device;
                    let (tex, view) = Self::create_offscreen_texture(device, new_w, new_h);
                    self.offscreen_texture = tex;
                    self.offscreen_view = view;

                    // Update egui texture handle
                    if let Some(tid) = self.texture_id {
                        let mut renderer = render_state.renderer.write();
                        renderer.update_egui_texture_from_wgpu_texture(
                            device,
                            &self.offscreen_view,
                            wgpu::FilterMode::Linear,
                            tid,
                        );
                    }
                }
                self.needs_repaint = true;
            }

            // L4: camera interaction on the preview image
            let size = egui::vec2(self.render_width as f32, self.render_height as f32);

            if let Some(texture_id) = self.texture_id {
                let response = ui.image(egui::load::SizedTexture::new(texture_id, size));
                let response = response.interact(egui::Sense::click_and_drag());

                // Orbit: left drag
                if response.dragged_by(egui::PointerButton::Primary) {
                    let delta = response.drag_delta();
                    self.camera.yaw -= delta.x * 0.005;
                    self.camera.pitch += delta.y * 0.005;
                    self.camera.pitch = self.camera.pitch.clamp(
                        -std::f32::consts::FRAC_PI_2 + 0.01,
                        std::f32::consts::FRAC_PI_2 - 0.01,
                    );
                    self.needs_repaint = true;
                }

                // Pan: middle drag
                if response.dragged_by(egui::PointerButton::Middle) {
                    let delta = response.drag_delta();
                    let cos_y = self.camera.yaw.cos();
                    let sin_y = self.camera.yaw.sin();
                    // right vector in xz-plane
                    let right = [cos_y, 0.0, -sin_y];
                    let up = [0.0, 1.0, 0.0];
                    let scale = self.camera.distance * 0.002;
                    for i in 0..3 {
                        self.camera.target[i] -= right[i] * delta.x * scale;
                        self.camera.target[i] += up[i] * delta.y * scale;
                    }
                    self.needs_repaint = true;
                }

                // Zoom: scroll wheel
                if response.hovered() {
                    let scroll = ctx.input(|i| i.smooth_scroll_delta.y);
                    if scroll != 0.0 {
                        self.camera.distance *= (1.0_f32 - scroll * 0.002).max(0.01);
                        self.needs_repaint = true;
                    }
                }
            }
        });

        // ---------------------------------------------------------------
        // Render offscreen
        // ---------------------------------------------------------------
        if let Some(render_state) = frame.wgpu_render_state() {
            self.render_to_texture(render_state);
        }

        // L7: request repaint only when needed
        if self.needs_repaint || matches!(self.bake_status, BakeStatus::Running) {
            ctx.request_repaint();
        }
    }
}
