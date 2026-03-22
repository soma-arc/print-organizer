/// End-to-end tests for the full sdf-baker pipeline.
///
/// These tests require genmesh to be built. If GENMESH_PATH env var is not set,
/// the tests look for genmesh at the default build location.
use std::fs;
use std::path::PathBuf;

use sdf_baker::bricks_writer::{write_bricks, write_manifest};
use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
use sdf_baker::genmesh_runner::{GenmeshRunConfig, run_genmesh};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{BUILTIN_SPHERE_SDF, compose_wgsl};
use sdf_baker::types::BakeConfig;

fn find_genmesh() -> Option<PathBuf> {
    // Check env var first
    if let Ok(path) = std::env::var("GENMESH_PATH") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Default build locations (relative to workspace root)
    let candidates = [
        "tools/genmesh/build/RelWithDebInfo/genmesh.exe",
        "tools/genmesh/build/Debug/genmesh.exe",
        "tools/genmesh/build/Release/genmesh.exe",
        "tools/genmesh/build/genmesh.exe",
        "tools/genmesh/build/genmesh",
    ];

    for candidate in &candidates {
        // Try from workspace root (2 levels up from crate dir)
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(candidate);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

fn run_full_pipeline(out_dir: &std::path::Path) -> sdf_baker::genmesh_runner::GenmeshResult {
    let ctx = init_gpu().expect("GPU init");
    let shader_src = compose_wgsl(BUILTIN_SPHERE_SDF);
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &shader_src, "cs_main").expect("pipeline creation");
    let config = BakeConfig::new(
        [0.0, 0.0, 0.0],
        [64.0, 64.0, 64.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    );

    let bricks = bake_all_bricks(&ctx, &pipeline, &layout, &config).expect("bake");
    write_manifest(out_dir, &config).expect("write manifest");
    write_bricks(out_dir, &config, &bricks).expect("write bricks");

    let genmesh_path =
        find_genmesh().expect("genmesh not found — set GENMESH_PATH or build genmesh");

    let genmesh_config = GenmeshRunConfig {
        genmesh_path,
        out_dir: out_dir.to_path_buf(),
        iso: 0.0,
        adaptivity: 0.0,
        write_vdb: false,
    };

    run_genmesh(&genmesh_config).expect("genmesh execution")
}

#[test]
fn test_e2e_mesh_stl_created() {
    if find_genmesh().is_none() {
        eprintln!("SKIP: genmesh not found");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    run_full_pipeline(dir.path());
    let stl_path = dir.path().join("mesh.stl");
    assert!(stl_path.exists(), "mesh.stl should be created");
    let stl_size = fs::metadata(&stl_path).unwrap().len();
    assert!(stl_size > 0, "mesh.stl should not be empty");
}

#[test]
fn test_e2e_report_json_success() {
    if find_genmesh().is_none() {
        eprintln!("SKIP: genmesh not found");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = run_full_pipeline(dir.path());

    assert!(result.report.is_some(), "report should be parsed");
    let report = result.report.unwrap();
    assert_eq!(report.status, "success");
    assert_eq!(report.stage, "write");
}

#[test]
fn test_e2e_triangle_count() {
    if find_genmesh().is_none() {
        eprintln!("SKIP: genmesh not found");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = run_full_pipeline(dir.path());
    let report = result.report.unwrap();

    // Sphere baseline: 24672 triangles
    assert_eq!(
        report.stats.triangle_count, 24672,
        "Expected 24672 triangles for built-in sphere SDF"
    );
    assert!(report.stats.vertex_count > 0);
}

#[test]
fn test_e2e_all_output_files_exist() {
    if find_genmesh().is_none() {
        eprintln!("SKIP: genmesh not found");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    run_full_pipeline(dir.path());

    let expected_files = [
        "manifest.json",
        "bricks.bin",
        "bricks.index.json",
        "mesh.stl",
        "report.json",
    ];
    for f in &expected_files {
        assert!(
            dir.path().join(f).exists(),
            "Expected output file '{f}' not found"
        );
    }
}

#[test]
fn test_e2e_report_timing() {
    if find_genmesh().is_none() {
        eprintln!("SKIP: genmesh not found");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let result = run_full_pipeline(dir.path());
    let report = result.report.unwrap();

    assert!(
        report.timing_ms.total > 0.0,
        "Total timing should be positive"
    );
}
