/// Integration tests for config file loading and the --config pipeline.
use std::path::PathBuf;

use sdf_baker::cli::Cli;
use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
use sdf_baker::config::{load_config, resolve_config};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{compose_shader, load_shader};

/// Workspace root (two levels up from crate manifest dir).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn example_path(name: &str) -> PathBuf {
    workspace_root()
        .join("examples")
        .join(name)
        .join(format!("{name}.json"))
}

/// Helper: build Cli with --out only (no --config).
fn cli_with_out(out: &str) -> Cli {
    Cli {
        config: None,
        shader: None,
        out: Some(PathBuf::from(out)),
        aabb_min: [0.0, 0.0, 0.0],
        aabb_size: [64.0, 64.0, 64.0],
        voxel_size: 1.0,
        brick_size: 64,
        half_width: 3,
        iso: 0.0,
        adaptivity: 0.0,
        dtype: "f32".to_string(),
        genmesh_path: None,
        skip_genmesh: false,
        write_vdb: false,
        force: false,
        log_level: "info".to_string(),
    }
}

#[test]
fn test_config_sphere_example_loads() {
    let config_path = example_path("sphere");
    let cfg = load_config(&config_path).unwrap();
    assert_eq!(cfg.shader.as_deref(), Some("sphere.wgsl"));
    assert_eq!(cfg.grid.brick_size, Some(64));
}

#[test]
fn test_config_gyroid_example_loads() {
    let config_path = example_path("gyroid");
    let cfg = load_config(&config_path).unwrap();
    assert_eq!(cfg.shader.as_deref(), Some("gyroid.wgsl"));
    assert_eq!(cfg.grid.aabb_min, Some([-64.0, -64.0, -64.0]));
    assert_eq!(cfg.grid.aabb_size, Some([128.0, 128.0, 128.0]));
}

#[test]
fn test_config_csg_example_loads() {
    let config_path = example_path("csg");
    let cfg = load_config(&config_path).unwrap();
    assert_eq!(cfg.shader.as_deref(), Some("csg.wgsl"));
}

#[test]
fn test_config_linked_torus_example_loads() {
    let config_path = example_path("linked-torus");
    let cfg = load_config(&config_path).unwrap();
    assert_eq!(cfg.shader.as_deref(), Some("linked-torus.wgsl"));
    assert_eq!(cfg.genmesh.write_vdb, Some(true));
}

#[test]
fn test_resolve_sphere_example() {
    let config_path = example_path("sphere");
    let cli = cli_with_out("test_out");
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

    // shader should be resolved relative to config dir
    let expected_shader = config_path.parent().unwrap().join("sphere.wgsl");
    assert_eq!(resolved.shader, Some(expected_shader));
    assert_eq!(resolved.bake_config.brick_size, 64);
    assert_eq!(resolved.bake_config.aabb_size, [64.0, 64.0, 64.0]);
}

#[test]
fn test_resolve_gyroid_example() {
    let config_path = example_path("gyroid");
    let cli = cli_with_out("test_out");
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

    assert_eq!(resolved.bake_config.aabb_min, [-64.0, -64.0, -64.0]);
    assert_eq!(resolved.bake_config.aabb_size, [128.0, 128.0, 128.0]);
    assert_eq!(resolved.bake_config.dims, [128, 128, 128]);
    assert_eq!(resolved.bake_config.brick_counts(), [2, 2, 2]);
}

#[test]
fn test_config_out_from_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.json");
    std::fs::write(&config_path, r#"{ "out": "my_output_dir" }"#).unwrap();

    // CLI has no --out, config has out (resolved relative to config dir)
    let mut cli = cli_with_out("_unused");
    cli.out = None;
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();
    assert_eq!(resolved.out, dir.path().join("my_output_dir"));
}

#[test]
fn test_config_no_out_anywhere_fails() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("test.json");
    std::fs::write(&config_path, "{}").unwrap();

    let mut cli = cli_with_out("_unused");
    cli.out = None;
    let result = resolve_config(&cli, Some(&config_path));
    assert!(result.is_err());
}

#[test]
fn test_sphere_example_gpu_bake() {
    let config_path = example_path("sphere");
    let cli = cli_with_out("test_out");
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

    let shader_path = resolved.shader.as_ref().unwrap();
    let (lang, user_sdf) = load_shader(shader_path).unwrap();
    let composed = compose_shader(lang, &user_sdf).unwrap();

    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &resolved.bake_config).unwrap();
    // 64^3 grid, brick_size=64 → 1 brick
    assert_eq!(bricks.len(), 1);
    assert!(!bricks[0].is_background);
    // Center should be negative (inside sphere)
    let center_idx = 32 + 64 * (32 + 64 * 32);
    assert!(bricks[0].values[center_idx] < 0.0);
}

#[test]
fn test_gyroid_example_gpu_bake() {
    let config_path = example_path("gyroid");
    let cli = cli_with_out("test_out");
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

    let shader_path = resolved.shader.as_ref().unwrap();
    let (lang, user_sdf) = load_shader(shader_path).unwrap();
    let composed = compose_shader(lang, &user_sdf).unwrap();

    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &resolved.bake_config).unwrap();
    // 128^3 grid, brick_size=64 → 8 bricks
    assert_eq!(bricks.len(), 8);
    // Gyroid fills entire space — all bricks should be active
    for brick in &bricks {
        assert!(
            !brick.is_background,
            "Gyroid brick ({},{},{}) should be active",
            brick.bx, brick.by, brick.bz
        );
    }
}

#[test]
fn test_csg_example_gpu_bake() {
    let config_path = example_path("csg");
    let cli = cli_with_out("test_out");
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

    let shader_path = resolved.shader.as_ref().unwrap();
    let (lang, user_sdf) = load_shader(shader_path).unwrap();
    let composed = compose_shader(lang, &user_sdf).unwrap();

    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &resolved.bake_config).unwrap();
    // 128^3 grid → 8 bricks, but CSG is centered, so some corners might be background
    assert!(bricks.len() <= 8);
    let active = bricks.iter().filter(|b| !b.is_background).count();
    assert!(active >= 1, "At least one active brick for CSG");
}

#[test]
fn test_linked_torus_example_gpu_bake() {
    let config_path = example_path("linked-torus");
    let cli = cli_with_out("test_out");
    let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

    let shader_path = resolved.shader.as_ref().unwrap();
    let (lang, user_sdf) = load_shader(shader_path).unwrap();
    let composed = compose_shader(lang, &user_sdf).unwrap();

    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &resolved.bake_config).unwrap();
    assert!(bricks.len() <= 8);
    let active = bricks.iter().filter(|b| !b.is_background).count();
    assert!(active >= 1, "At least one active brick for linked torus");
    // Torus is smaller than the grid, so some bricks may be background.
    // With half_width=3, background_value=3.0, but corner distance to torus
    // may be less — this depends on geometry. Just verify the pipeline runs.
}
