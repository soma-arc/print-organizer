use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Instant;

use eframe::{egui, wgpu};

mod bake;
mod camera;
mod config_info;
mod renderer;

use bake::{BakeResult, spawn_bake};
use camera::OrbitCamera;
use config_info::ConfigInfo;

// ---------------------------------------------------------------------------
// BakeStatus — UI state machine (kept in mod.rs)
// ---------------------------------------------------------------------------

/// Status of the bake pipeline.
#[derive(Debug)]
enum BakeStatus {
    Idle,
    Running,
    Done(BakeResult),
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

        let render_width = 800;
        let render_height = 600;

        let camera = OrbitCamera {
            target: [0.0, 0.0, 0.0],
            yaw: std::f32::consts::FRAC_PI_4,
            pitch: 0.5236,
            distance: 100.0,
        };

        let gpu = renderer::init_gpu_resources(render_state, &camera, render_width, render_height);

        Self {
            bind_group_layout: gpu.bind_group_layout,
            render_pipeline: gpu.render_pipeline,
            globals_buffer: gpu.globals_buffer,
            globals_bind_group: gpu.globals_bind_group,
            offscreen_texture: gpu.offscreen_texture,
            offscreen_view: gpu.offscreen_view,
            texture_id: Some(gpu.texture_id),
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
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.out_dir_override);
                        let can_open = !self.out_dir_override.is_empty()
                            && std::path::Path::new(&self.out_dir_override).exists();
                        if ui
                            .add_enabled(can_open, egui::Button::new("📂"))
                            .on_hover_text("フォルダを開く")
                            .clicked()
                        {
                            let _ = opener::open(&self.out_dir_override);
                        }
                    });
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
                    if ui.checkbox(&mut self.clip_aabb, "AABB クリップ").changed() {
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
                    let (tex, view) = renderer::create_offscreen_texture(device, new_w, new_h);
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
