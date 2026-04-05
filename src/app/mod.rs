use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use eframe::{egui, wgpu};
use notify::Watcher as _;
use sdf_baker::shader_compose::{ShaderDiagnostic, ShaderDiagnostics};

mod bake;
mod camera;
mod config_info;
mod renderer;

use bake::{BakeResult, spawn_bake};
use camera::OrbitCamera;
use config_info::ConfigInfo;

/// Convert a preset name into a filesystem-safe slug.
fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .to_ascii_lowercase()
}

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
    /// Errors from the last shader compilation attempt
    shader_errors: Vec<ShaderDiagnostic>,
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
    config: Option<sdf_baker::config::ConfigFile>,
    config_dir: Option<PathBuf>,
    config_info: Option<ConfigInfo>,
    config_error: Option<String>,
    /// Currently selected preset index (None = base config).
    selected_preset: Option<usize>,

    out_dir_override: String,
    force_overwrite: bool,

    bake_status: BakeStatus,
    bake_rx: Option<mpsc::Receiver<BakeResult>>,

    /// Channel for file dialog results (path selected by user)
    file_rx: Option<mpsc::Receiver<PathBuf>>,

    // --- shader hot-reload ---
    /// File watcher for the current shader file
    _watcher: Option<notify::RecommendedWatcher>,
    /// Receives notifications from the file watcher
    watcher_rx: Option<mpsc::Receiver<()>>,
    /// Debounce: timestamp of last watcher event
    pending_reload: Option<Instant>,
    /// Resolved absolute path of the currently loaded shader file
    resolved_shader_path: Option<PathBuf>,
    /// egui context for requesting repaints from background threads
    egui_ctx: Option<egui::Context>,

    // --- config hot-reload ---
    /// File watcher for the config JSON
    _config_watcher: Option<notify::RecommendedWatcher>,
    /// Receives notifications from the config file watcher
    config_watcher_rx: Option<mpsc::Receiver<()>>,
    /// Debounce: timestamp of last config watcher event
    pending_config_reload: Option<Instant>,
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
            shader_errors: Vec::new(),
            needs_repaint: false,
            show_aabb: true,
            show_bricks: false,
            clip_aabb: true,

            config_path: None,
            config: None,
            config_dir: None,
            config_info: None,
            config_error: None,
            selected_preset: None,
            out_dir_override: String::new(),
            force_overwrite: true,
            bake_status: BakeStatus::Idle,
            bake_rx: None,
            file_rx: None,

            _watcher: None,
            watcher_rx: None,
            pending_reload: None,
            resolved_shader_path: None,
            egui_ctx: None,

            _config_watcher: None,
            config_watcher_rx: None,
            pending_config_reload: None,
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
                            self.set_shader_error(e);
                        }
                    }
                    self.start_watching_shader(&shader_path);
                } else {
                    // No shader specified — use built-in sphere
                    self.rebuild_preview_pipeline(
                        device,
                        sdf_baker::shader_compose::ShaderLang::Wgsl,
                        sdf_baker::shader_compose::BUILTIN_SPHERE_SDF,
                    );
                    self.stop_watching_shader();
                }

                self.config = Some(cfg);
                self.config_dir = Some(cfg_dir);
                self.config_info = Some(info);
                self.config_error = None;
                self.selected_preset = None;
            }
            Err(e) => {
                self.config = None;
                self.config_dir = None;
                self.config_info = None;
                self.config_error = Some(format!("{e:#}"));
                self.selected_preset = None;
                self.stop_watching_shader();
            }
        }
        self.config_path = Some(path.clone());
        self.start_watching_config(&path);
        self.bake_status = BakeStatus::Idle;
        self.needs_repaint = true;
    }

    /// Start watching the config JSON file for changes.
    /// Drops any previously active config watcher first.
    fn start_watching_config(&mut self, config_path: &std::path::Path) {
        self.stop_watching_config();

        let (tx, rx) = mpsc::channel();
        let sender = std::sync::Mutex::new(tx);
        let watch_path = config_path.to_path_buf();
        let filter_path = watch_path.clone();
        let repaint_ctx = self.egui_ctx.clone();

        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                use notify::EventKind::*;
                match event.kind {
                    Modify(_) | Create(_) | Remove(_) => {
                        if event.paths.iter().any(|p| p == &filter_path) {
                            let _ = sender.lock().unwrap().send(());
                            if let Some(ref ctx) = repaint_ctx {
                                ctx.request_repaint();
                            }
                        }
                    }
                    _ => {}
                }
            }
        });

        match watcher {
            Ok(mut w) => {
                let watch_dir = watch_path.parent().unwrap_or(std::path::Path::new("."));
                if w.watch(watch_dir, notify::RecursiveMode::NonRecursive).is_ok() {
                    self._config_watcher = Some(w);
                    self.config_watcher_rx = Some(rx);
                    self.pending_config_reload = None;
                    log::info!("Watching config: {}", config_path.display());
                } else {
                    log::warn!("Failed to watch config directory: {}", watch_dir.display());
                }
            }
            Err(e) => {
                log::warn!("Failed to create config file watcher: {e}");
            }
        }
    }

    /// Stop watching the config JSON file.
    fn stop_watching_config(&mut self) {
        self._config_watcher = None;
        self.config_watcher_rx = None;
        self.pending_config_reload = None;
    }

    /// Start watching the shader file for changes.
    /// Drops any previously active watcher.
    fn start_watching_shader(&mut self, shader_path: &std::path::Path) {
        // Always drop the previous watcher first
        self.stop_watching_shader();

        let (tx, rx) = mpsc::channel();
        let sender = std::sync::Mutex::new(tx);
        let watch_path = shader_path.to_path_buf();
        let filter_path = watch_path.clone();
        let repaint_ctx = self.egui_ctx.clone();

        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                use notify::EventKind::*;
                match event.kind {
                    Modify(_) | Create(_) | Remove(_) => {
                        // Only trigger if the event involves the watched shader file
                        if event.paths.iter().any(|p| p == &filter_path) {
                            let _ = sender.lock().unwrap().send(());
                            // Wake the event loop even when the window is unfocused
                            if let Some(ref ctx) = repaint_ctx {
                                ctx.request_repaint();
                            }
                        }
                    }
                    _ => {}
                }
            }
        });

        match watcher {
            Ok(mut w) => {
                // Watch the parent directory to catch rename-based atomic saves
                let watch_dir = watch_path.parent().unwrap_or(std::path::Path::new("."));
                if w.watch(watch_dir, notify::RecursiveMode::NonRecursive)
                    .is_ok()
                {
                    self._watcher = Some(w);
                    self.watcher_rx = Some(rx);
                    self.resolved_shader_path = Some(watch_path);
                    self.pending_reload = None;
                    log::info!("Watching shader: {}", shader_path.display());
                } else {
                    log::warn!("Failed to watch directory: {}", watch_dir.display());
                }
            }
            Err(e) => {
                log::warn!("Failed to create file watcher: {e}");
            }
        }
    }

    /// Stop watching the shader file.
    fn stop_watching_shader(&mut self) {
        self._watcher = None;
        self.watcher_rx = None;
        self.pending_reload = None;
        self.resolved_shader_path = None;
    }

    /// Reload the shader from `resolved_shader_path` and rebuild the pipeline.
    fn reload_shader(&mut self, device: &wgpu::Device) {
        let Some(shader_path) = self.resolved_shader_path.clone() else {
            return;
        };
        match sdf_baker::shader_compose::load_shader(&shader_path) {
            Ok((lang, user_sdf)) => {
                self.rebuild_preview_pipeline(device, lang, &user_sdf);
                log::info!("Shader reloaded: {}", shader_path.display());
            }
            Err(e) => {
                self.set_shader_error(e);
            }
        }
        self.needs_repaint = true;
    }

    /// Switch preset selection and update config_info / preview accordingly.
    fn apply_preset(&mut self, preset_index: Option<usize>, device: &wgpu::Device) {
        let Some(base) = &self.config else { return };
        let Some(cfg_dir) = &self.config_dir else { return };

        let effective = match preset_index {
            Some(idx) => {
                let presets = match &base.presets {
                    Some(p) if idx < p.len() => p,
                    _ => return,
                };
                sdf_baker::config::merge_preset(base, &presets[idx])
            }
            None => base.clone(),
        };

        let old_info = self.config_info.as_ref();
        let new_info = ConfigInfo::from_config(&effective, cfg_dir);

        // Check if shader changed
        let shader_changed = old_info.map(|i| &i.shader) != Some(&new_info.shader);
        // Check if AABB changed
        let aabb_changed = old_info.map(|i| (i.aabb_min, i.aabb_size))
            != Some((new_info.aabb_min, new_info.aabb_size));

        // Update output directory from effective config
        self.out_dir_override = effective
            .out
            .as_ref()
            .map(|out| cfg_dir.join(out).display().to_string())
            .unwrap_or_default();

        if aabb_changed {
            self.camera = OrbitCamera::from_aabb(new_info.aabb_min, new_info.aabb_size);
        }

        if shader_changed {
            if let Some(ref shader_rel) = effective.shader {
                let shader_path = cfg_dir.join(shader_rel);
                match sdf_baker::shader_compose::load_shader(&shader_path) {
                    Ok((lang, user_sdf)) => {
                        self.rebuild_preview_pipeline(device, lang, &user_sdf);
                    }
                    Err(e) => {
                        self.set_shader_error(e);
                    }
                }
                self.start_watching_shader(&shader_path);
            } else {
                self.rebuild_preview_pipeline(
                    device,
                    sdf_baker::shader_compose::ShaderLang::Wgsl,
                    sdf_baker::shader_compose::BUILTIN_SPHERE_SDF,
                );
                self.stop_watching_shader();
            }
        }

        self.config_info = Some(new_info);
        self.selected_preset = preset_index;
        self.needs_repaint = true;
    }

    /// Get the effective (merged) ConfigFile for the current selection.
    fn effective_config(&self) -> Option<sdf_baker::config::ConfigFile> {
        let base = self.config.as_ref()?;
        match self.selected_preset {
            Some(idx) => {
                let presets = base.presets.as_ref()?;
                Some(sdf_baker::config::merge_preset(base, presets.get(idx)?))
            }
            None => Some(base.clone()),
        }
    }

    /// Extract structured diagnostics from an anyhow error and store them.
    fn set_shader_error(&mut self, err: anyhow::Error) {
        if let Some(diags) = err.downcast_ref::<ShaderDiagnostics>() {
            self.shader_errors = diags.diagnostics.clone();
        } else {
            self.shader_errors = vec![ShaderDiagnostic {
                line: None,
                column: None,
                message: format!("{err:#}"),
            }];
        }
        self.preview_active = false;
    }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.needs_repaint = false;

        // Store context so background threads (e.g. file watcher) can wake us
        if self.egui_ctx.is_none() {
            self.egui_ctx = Some(ctx.clone());
        }

        // ---------------------------------------------------------------
        // Grab device reference for pipeline operations
        // ---------------------------------------------------------------
        let device: Option<wgpu::Device> = frame.wgpu_render_state().map(|rs| rs.device.clone());

        // ---------------------------------------------------------------
        // Poll async channels
        // ---------------------------------------------------------------

        // Shader hot-reload (debounced)
        if let Some(rx) = &self.watcher_rx {
            if rx.try_recv().is_ok() {
                // Drain any additional queued events
                while rx.try_recv().is_ok() {}
                self.pending_reload = Some(Instant::now());
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }
        if let Some(t) = self.pending_reload {
            if t.elapsed() >= Duration::from_millis(200) {
                self.pending_reload = None;
                if let Some(ref device) = device {
                    self.reload_shader(device);
                }
            }
        }

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
        let prev_preset = self.selected_preset;
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

                // --- Preset selector (v2 only) ---
                if let Some(ref cfg) = self.config {
                    if let Some(ref presets) = cfg.presets {
                        if !presets.is_empty() {
                            let current_label = match self.selected_preset {
                                Some(idx) => presets
                                    .get(idx)
                                    .map(|p| p.name.as_str())
                                    .unwrap_or("(base)"),
                                None => "(base)",
                            };
                            ui.horizontal(|ui| {
                                ui.label("Preset:");
                                egui::ComboBox::from_id_salt("preset_selector")
                                    .selected_text(current_label)
                                    .show_ui(ui, |ui| {
                                        if ui
                                            .selectable_label(
                                                self.selected_preset.is_none(),
                                                "(base)",
                                            )
                                            .clicked()
                                            && self.selected_preset.is_some()
                                        {
                                            self.selected_preset = None;
                                        }
                                        for (i, preset) in presets.iter().enumerate() {
                                            if ui
                                                .selectable_label(
                                                    self.selected_preset == Some(i),
                                                    &preset.name,
                                                )
                                                .clicked()
                                                && self.selected_preset != Some(i)
                                            {
                                                self.selected_preset = Some(i);
                                            }
                                        }
                                    });
                            });
                        }
                    }
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

                        ui.label("Offset (mm):");
                        ui.label(format!("{}", info.offset_mm));
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
                            let effective = self.effective_config().unwrap();
                            let config_dir = self.config_dir.clone().unwrap();
                            let base_out = PathBuf::from(&self.out_dir_override);

                            // Determine output directory based on preset selection
                            let out_dir = if let Some(ref base_cfg) = self.config {
                                let has_presets = base_cfg
                                    .presets
                                    .as_ref()
                                    .is_some_and(|p| !p.is_empty());
                                if has_presets {
                                    if let Some(idx) = self.selected_preset {
                                        let preset = &base_cfg.presets.as_ref().unwrap()[idx];
                                        if preset.out.is_some() {
                                            // Preset has explicit out — already in
                                            // out_dir_override
                                            base_out
                                        } else {
                                            base_out.join(slug(&preset.name))
                                        }
                                    } else {
                                        base_out.join("default")
                                    }
                                } else {
                                    // v1: direct output
                                    base_out
                                }
                            } else {
                                base_out
                            };

                            let force = self.force_overwrite;

                            let (tx, rx) = mpsc::channel();
                            self.bake_rx = Some(rx);
                            self.bake_status = BakeStatus::Running;

                            spawn_bake(effective, config_dir, out_dir, force, tx);
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

                    // --- Shader errors ---
                    if !self.shader_errors.is_empty() {
                        ui.separator();
                        ui.colored_label(
                            egui::Color32::YELLOW,
                            format!("⚠ Shader errors ({})", self.shader_errors.len()),
                        );
                        for diag in &self.shader_errors {
                            ui.colored_label(egui::Color32::YELLOW, format!("  {diag}"));
                        }
                    }
                }
            });

        // Apply preset change if selection changed during UI draw
        if self.selected_preset != prev_preset {
            if let Some(ref device) = device {
                self.apply_preset(self.selected_preset, device);
            }
        }

        // ---------------------------------------------------------------
        // Central panel — 3D preview
        // ---------------------------------------------------------------
        egui::CentralPanel::default().show(ctx, |ui| {
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
        if self.needs_repaint
            || matches!(self.bake_status, BakeStatus::Running)
            || self.pending_reload.is_some()
        {
            ctx.request_repaint();
        }
    }
}
