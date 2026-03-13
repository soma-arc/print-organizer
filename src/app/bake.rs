use std::path::PathBuf;
use std::time::Instant;

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

/// Run the full sdf-baker pipeline on a background thread.
pub(super) fn spawn_bake(
    config_path: PathBuf,
    out_dir: PathBuf,
    force: bool,
    tx: std::sync::mpsc::Sender<BakeResult>,
) {
    std::thread::spawn(move || {
        let result = run_bake_pipeline(&config_path, &out_dir, force);
        let _ = tx.send(result);
    });
}

fn run_bake_pipeline(config_path: &PathBuf, out_dir: &PathBuf, force: bool) -> BakeResult {
    use sdf_baker::bricks_writer::{write_bricks, write_manifest};
    use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
    use sdf_baker::config::{load_config, resolve_genmesh_path};
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
    let genmesh_path = cfg.genmesh.path.as_ref().map(|p| cfg_dir.join(p));

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
        let genmesh_exe = resolve_genmesh_path(genmesh_path);

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
