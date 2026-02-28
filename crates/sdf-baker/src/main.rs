use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use sdf_baker::bricks_writer::{write_bricks, write_manifest};
use sdf_baker::cli::Cli;
use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
use sdf_baker::genmesh_runner::{run_genmesh, GenmeshRunConfig};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{compose_shader, load_shader, ShaderLang, BUILTIN_SPHERE_SDF};
use sdf_baker::types::BakeConfig;

fn main() {
    let cli = Cli::parse();

    // Initialize logger
    env_logger::Builder::new()
        .filter_level(match cli.log_level.as_str() {
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "debug" => log::LevelFilter::Debug,
            _ => log::LevelFilter::Info,
        })
        .init();

    log::info!("sdf-baker v{}", env!("CARGO_PKG_VERSION"));
    log::debug!("{cli:#?}");

    if let Err(e) = run_pipeline(&cli) {
        log::error!("Pipeline failed: {e:#}");
        std::process::exit(1);
    }
}

fn run_pipeline(cli: &Cli) -> Result<()> {
    let total_start = Instant::now();

    // 1. Prepare output directory
    if cli.out.exists() && !cli.force {
        anyhow::bail!(
            "Output directory already exists: {}. Use --force to overwrite.",
            cli.out.display()
        );
    }
    std::fs::create_dir_all(&cli.out)?;

    // 2. GPU device init
    log::info!("Initializing GPU...");
    let gpu_start = Instant::now();
    let ctx = init_gpu()?;
    log::info!("GPU init: {:.1}ms", gpu_start.elapsed().as_secs_f64() * 1000.0);

    // 3. Load shader
    let (lang, user_sdf) = if let Some(ref shader_path) = cli.shader {
        log::info!("Loading shader: {}", shader_path.display());
        load_shader(shader_path)?
    } else {
        log::info!("Using built-in sphere SDF");
        (ShaderLang::Wgsl, BUILTIN_SPHERE_SDF.to_string())
    };

    // 4. Compose + compile shader
    log::info!("Composing shader ({:?})...", lang);
    let composed = compose_shader(lang, &user_sdf)?;
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point)?;

    // 5. Build config
    let config = BakeConfig::new(
        cli.aabb_min,
        cli.aabb_size,
        cli.voxel_size,
        cli.brick_size,
        cli.half_width,
        cli.iso,
        cli.adaptivity,
        cli.dtype.clone(),
    );
    log::info!(
        "Grid: dims={:?}, brick_size={}, bricks={:?}",
        config.dims,
        config.brick_size,
        config.brick_counts()
    );

    // 6. Bake all bricks
    log::info!("Baking SDF...");
    let bake_start = Instant::now();
    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config)?;
    let bake_time = bake_start.elapsed();
    log::info!("Bake complete: {:.1}ms", bake_time.as_secs_f64() * 1000.0);

    // 7. Write bricks output
    log::info!("Writing bricks to {}...", cli.out.display());
    write_manifest(&cli.out, &config)?;
    write_bricks(&cli.out, &config, &bricks)?;

    // 8. Run genmesh (unless skipped)
    if cli.skip_genmesh {
        log::info!("Skipping genmesh (--skip-genmesh)");
    } else {
        let genmesh_path = cli.genmesh_path.clone().unwrap_or_else(|| {
            // Default: look for genmesh next to sdf-baker or in PATH
            std::path::PathBuf::from("genmesh")
        });

        let genmesh_config = GenmeshRunConfig {
            genmesh_path,
            out_dir: cli.out.clone(),
            iso: cli.iso,
            adaptivity: cli.adaptivity,
            write_vdb: cli.write_vdb,
        };

        let result = run_genmesh(&genmesh_config)?;

        if let Some(ref report) = result.report {
            log::info!(
                "Mesh: {} triangles, {} vertices",
                report.stats.triangle_count,
                report.stats.vertex_count,
            );
        }
    }

    // 9. Summary
    let total_time = total_start.elapsed();
    log::info!("Done in {:.1}ms", total_time.as_secs_f64() * 1000.0);

    Ok(())
}
