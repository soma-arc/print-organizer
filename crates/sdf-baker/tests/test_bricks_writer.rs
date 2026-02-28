use std::fs;

use sdf_baker::bricks_writer::{write_bricks, write_manifest};
use sdf_baker::compute::{bake_all_bricks, create_compute_pipeline};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{compose_wgsl, BUILTIN_SPHERE_SDF};
use sdf_baker::types::BakeConfig;

fn setup_and_bake() -> (BakeConfig, Vec<sdf_baker::types::BrickResult>) {
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
    (config, bricks)
}

#[test]
fn test_bake_and_write_manifest() {
    let (config, _bricks) = setup_and_bake();
    let dir = tempfile::tempdir().unwrap();
    let path = write_manifest(dir.path(), &config).unwrap();
    assert!(path.exists());

    let content = fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(v["version"], 1);
    assert_eq!(v["dims"], serde_json::json!([64, 64, 64]));
}

#[test]
fn test_bake_and_write_bricks() {
    let (config, bricks) = setup_and_bake();
    let dir = tempfile::tempdir().unwrap();
    write_bricks(dir.path(), &config, &bricks).unwrap();

    let bin_path = dir.path().join("bricks.bin");
    let index_path = dir.path().join("bricks.index.json");

    assert!(bin_path.exists());
    assert!(index_path.exists());

    // Sphere in 64^3 with brick_size=64 → 1 brick, should be active
    let active_count = bricks.iter().filter(|b| !b.is_background).count();
    assert!(active_count > 0, "At least one active brick expected");

    let bin_size = fs::metadata(&bin_path).unwrap().len();
    let expected_size = active_count as u64 * 64 * 64 * 64 * 4;
    assert_eq!(bin_size, expected_size);
}

#[test]
fn test_bake_and_write_full_pipeline() {
    let (config, bricks) = setup_and_bake();
    let dir = tempfile::tempdir().unwrap();

    write_manifest(dir.path(), &config).unwrap();
    write_bricks(dir.path(), &config, &bricks).unwrap();

    // All 3 files should exist
    assert!(dir.path().join("manifest.json").exists());
    assert!(dir.path().join("bricks.bin").exists());
    assert!(dir.path().join("bricks.index.json").exists());

    // Verify index references valid offsets
    let index_content = fs::read_to_string(dir.path().join("bricks.index.json")).unwrap();
    let index: serde_json::Value = serde_json::from_str(&index_content).unwrap();
    let brick_arr = index["bricks"].as_array().unwrap();

    let bin_size = fs::metadata(dir.path().join("bricks.bin")).unwrap().len();
    for entry in brick_arr {
        let offset = entry["offset_bytes"].as_u64().unwrap();
        let payload = entry["payload_bytes"].as_u64().unwrap();
        assert!(
            offset + payload <= bin_size,
            "Brick offset+payload exceeds bricks.bin size"
        );
    }
}

#[test]
fn test_bake_and_write_manifest_schema_fields() {
    let (config, _bricks) = setup_and_bake();
    let dir = tempfile::tempdir().unwrap();
    write_manifest(dir.path(), &config).unwrap();

    let content = fs::read_to_string(dir.path().join("manifest.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Verify all required fields from manifest.v1.schema.json exist
    let required = [
        "version",
        "coordinate_system",
        "units",
        "aabb_min",
        "aabb_size",
        "voxel_size",
        "dims",
        "sample_at",
        "axis_order",
        "distance_sign",
        "iso",
        "adaptivity",
        "narrow_band",
        "brick",
        "dtype",
        "background_value_mm",
        "hashes",
    ];
    for field in required {
        assert!(
            !v[field].is_null(),
            "Required field '{field}' is missing from manifest"
        );
    }
}
