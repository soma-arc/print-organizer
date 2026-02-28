/// Integration tests for multi-brick grids and sparse optimization (Phase R5).
use std::fs;
use std::path::PathBuf;

use sdf_baker::bricks_writer::{write_bricks, write_manifest};
use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
use sdf_baker::genmesh_runner::{run_genmesh, GenmeshRunConfig};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{compose_wgsl, BUILTIN_SPHERE_SDF};
use sdf_baker::types::BakeConfig;

fn find_genmesh() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("GENMESH_PATH") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }
    let candidates = [
        "tools/genmesh/build/Debug/genmesh.exe",
        "tools/genmesh/build/Release/genmesh.exe",
        "tools/genmesh/build/genmesh.exe",
        "tools/genmesh/build/genmesh",
    ];
    for candidate in &candidates {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(candidate);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

#[test]
fn test_multi_brick_128_grid_brick_count() {
    // 128^3 grid with brick_size=64 → 2x2x2 = 8 bricks
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [128.0, 128.0, 128.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );
    assert_eq!(config.brick_counts(), [2, 2, 2]);

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();
    assert_eq!(bricks.len(), 8, "Should have 8 bricks for 128^3 grid");

    // Each brick should have 64^3 values
    for brick in &bricks {
        assert_eq!(brick.values.len(), 64 * 64 * 64);
    }
}

#[test]
fn test_multi_brick_sparse_some_background() {
    // Sphere at (32,32,32) r=25.6 in a 128^3 grid
    // The sphere only occupies brick (0,0,0). Other bricks should have
    // at least some background bricks
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [128.0, 128.0, 128.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();

    let active_count = bricks.iter().filter(|b| !b.is_background).count();
    let bg_count = bricks.iter().filter(|b| b.is_background).count();

    assert!(
        active_count < bricks.len(),
        "Some bricks should be background (got {} active out of {})",
        active_count,
        bricks.len()
    );
    assert!(
        bg_count > 0,
        "At least one background brick expected (sphere at corner of 128^3 grid)"
    );
    assert!(
        active_count > 0,
        "At least one active brick expected (sphere should be present)"
    );

    // Brick (0,0,0) should be active (sphere center is at (32,32,32))
    let brick_000 = bricks
        .iter()
        .find(|b| b.bx == 0 && b.by == 0 && b.bz == 0)
        .unwrap();
    assert!(
        !brick_000.is_background,
        "Brick (0,0,0) should be active (contains sphere)"
    );
}

#[test]
fn test_multi_brick_sparse_bricks_bin_smaller() {
    // With sparse optimization, bricks.bin should be smaller than
    // total_bricks * brick_size^3 * 4 bytes
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [128.0, 128.0, 128.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();
    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();

    let dir = tempfile::tempdir().unwrap();
    write_manifest(dir.path(), &config).unwrap();
    write_bricks(dir.path(), &config, &bricks).unwrap();

    let bin_size = fs::metadata(dir.path().join("bricks.bin")).unwrap().len();
    let full_size = 8u64 * 64 * 64 * 64 * 4; // 8 bricks * 64^3 * sizeof(f32)
    let active_count = bricks.iter().filter(|b| !b.is_background).count();
    let expected_size = active_count as u64 * 64 * 64 * 64 * 4;

    assert_eq!(
        bin_size, expected_size,
        "bricks.bin should only contain active bricks"
    );
    assert!(
        bin_size < full_size,
        "bricks.bin ({bin_size}) should be smaller than full size ({full_size})"
    );
}

#[test]
fn test_multi_brick_index_only_active() {
    // bricks.index.json should only list active bricks
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [128.0, 128.0, 128.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();
    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();
    let active_count = bricks.iter().filter(|b| !b.is_background).count();

    let dir = tempfile::tempdir().unwrap();
    write_bricks(dir.path(), &config, &bricks).unwrap();

    let index_content = fs::read_to_string(dir.path().join("bricks.index.json")).unwrap();
    let index: serde_json::Value = serde_json::from_str(&index_content).unwrap();
    let index_bricks = index["bricks"].as_array().unwrap();

    assert_eq!(
        index_bricks.len(),
        active_count,
        "bricks.index.json should only list active bricks"
    );
}

#[test]
fn test_multi_brick_e2e_genmesh() {
    if find_genmesh().is_none() {
        eprintln!("SKIP: genmesh not found");
        return;
    }

    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [128.0, 128.0, 128.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();
    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();

    let dir = tempfile::tempdir().unwrap();
    write_manifest(dir.path(), &config).unwrap();
    write_bricks(dir.path(), &config, &bricks).unwrap();

    let genmesh_path = find_genmesh().unwrap();
    let result = run_genmesh(&GenmeshRunConfig {
        genmesh_path,
        out_dir: dir.path().to_path_buf(),
        iso: 0.0,
        adaptivity: 0.0,
        write_vdb: false,
    })
    .unwrap();

    assert!(result.report.is_some());
    let report = result.report.unwrap();
    assert_eq!(report.status, "success");
    assert!(
        report.stats.triangle_count > 0,
        "Multi-brick mesh should have triangles"
    );
    assert!(dir.path().join("mesh.stl").exists());
}

#[test]
fn test_multi_brick_non_uniform_grid() {
    // Non-uniform grid: 100x50x70 → dims=[100,50,70], bricks=[2,1,2]
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [100.0, 50.0, 70.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );
    assert_eq!(config.brick_counts(), [2, 1, 2]);

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();
    assert_eq!(bricks.len(), 4, "Should have 4 bricks for 100x50x70 grid");

    // No NaN values
    for brick in &bricks {
        assert!(
            brick.values.iter().all(|v| v.is_finite()),
            "All brick values should be finite"
        );
    }
}

#[test]
fn test_pipeline_reuse_across_bricks() {
    // Verify that using a single pipeline for all bricks works correctly.
    // This implicitly tests R5.3 (pipeline reuse).
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [128.0, 128.0, 128.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );

    let ctx = init_gpu().unwrap();
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) = create_compute_pipeline(&ctx.device, &shader_src, "cs_main").unwrap();

    // bake_all_bricks reuses the same pipeline for all 8 bricks
    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).unwrap();

    // Verify brick (0,0,0) has the sphere center negative
    let b000 = bricks.iter().find(|b| b.bx == 0 && b.by == 0 && b.bz == 0).unwrap();
    let center_idx = 32 + 64 * (32 + 64 * 32);
    assert!(
        b000.values[center_idx] < 0.0,
        "Sphere center should be negative in brick (0,0,0)"
    );

    // Brick (1,1,1) covers voxels [64..128] in all axes.
    // Sphere at (32,32,32) r=25.6 → max extent ~57.6 → brick (1,1,1) is all outside
    let b111 = bricks.iter().find(|b| b.bx == 1 && b.by == 1 && b.bz == 1).unwrap();
    assert!(
        b111.is_background,
        "Brick (1,1,1) should be background (far from sphere)"
    );
}
