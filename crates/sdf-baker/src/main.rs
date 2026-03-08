use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use sdf_baker::bricks_writer::{write_bricks, write_manifest};
use sdf_baker::cli::Cli;
use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
use sdf_baker::config::{resolve_config, resolve_genmesh_path};
use sdf_baker::genmesh_runner::{GenmeshRunConfig, run_genmesh};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{BUILTIN_SPHERE_SDF, ShaderLang, compose_shader, load_shader};

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

    // 0. Resolve config (merge config file + CLI args)
    let resolved = resolve_config(cli, cli.config.as_deref())?;
    let out = &resolved.out;
    let config = &resolved.bake_config;

    if let Some(ref config_path) = cli.config {
        log::info!("Config: {}", config_path.display());
    }

    // 1. Prepare output directory
    if out.exists() && !resolved.force {
        anyhow::bail!(
            "Output directory already exists: {}. Use --force to overwrite.",
            out.display()
        );
    }
    std::fs::create_dir_all(out)?;

    // 2. GPU device init
    log::info!("Initializing GPU...");
    let gpu_start = Instant::now();
    let ctx = init_gpu()?;
    log::info!(
        "GPU init: {:.1}ms",
        gpu_start.elapsed().as_secs_f64() * 1000.0
    );

    // 3. Load shader
    let (lang, user_sdf) = if let Some(ref shader_path) = resolved.shader {
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

    // 5. Config already built by resolve_config
    log::info!(
        "Grid: dims={:?}, brick_size={}, bricks={:?}",
        config.dims,
        config.brick_size,
        config.brick_counts()
    );

    // 6. Bake all bricks
    log::info!("Baking SDF...");
    let bake_start = Instant::now();
    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, config)?;
    let bake_time = bake_start.elapsed();
    log::info!("Bake complete: {:.1}ms", bake_time.as_secs_f64() * 1000.0);

    // 7. Write bricks output
    log::info!("Writing bricks to {}...", out.display());
    write_manifest(out, config)?;
    write_bricks(out, config, &bricks)?;

    // 8. Run genmesh (unless skipped)
    if resolved.skip_genmesh {
        log::info!("Skipping genmesh (--skip-genmesh)");
    } else {
        let genmesh_path = resolve_genmesh_path(resolved.genmesh_path.clone());

        let genmesh_config = GenmeshRunConfig {
            genmesh_path,
            out_dir: out.clone(),
            iso: config.iso,
            adaptivity: config.adaptivity,
            write_vdb: resolved.write_vdb,
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
