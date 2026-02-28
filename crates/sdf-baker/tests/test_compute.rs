use sdf_baker::compute::{bake_all_bricks, bake_brick, create_compute_pipeline};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{compose_wgsl, BUILTIN_SPHERE_SDF};
use sdf_baker::types::BakeConfig;

fn setup() -> (
    sdf_baker::gpu::GpuContext,
    wgpu::ComputePipeline,
    wgpu::BindGroupLayout,
    BakeConfig,
) {
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
    (ctx, pipeline, layout, config)
}

#[test]
fn test_bake_single_brick_value_count() {
    let (ctx, pipeline, layout, config) = setup();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0)
        .expect("bake_brick");
    // 64^3 = 262144 values
    assert_eq!(values.len(), 64 * 64 * 64);
}

#[test]
fn test_bake_single_brick_center_negative() {
    let (ctx, pipeline, layout, config) = setup();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0)
        .expect("bake_brick");

    // The sphere SDF: length(p - (32,32,32)) - 25.6
    // At center (32,32,32): distance = 0 - 25.6 = -25.6 (inside)
    // x-fastest order: index = x + 64*(y + 64*z)
    let center_idx = 32 + 64 * (32 + 64 * 32);
    assert!(
        values[center_idx] < 0.0,
        "Center should be inside the sphere, got {}",
        values[center_idx]
    );
}

#[test]
fn test_bake_single_brick_corner_positive() {
    let (ctx, pipeline, layout, config) = setup();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0)
        .expect("bake_brick");

    // At corner (0,0,0) + 0.5 offset = (0.5,0.5,0.5):
    // distance = length((0.5,0.5,0.5) - (32,32,32)) - 25.6 ≈ 54.56 - 25.6 = 28.96 (outside)
    let corner_idx = 0; // (0,0,0)
    assert!(
        values[corner_idx] > 0.0,
        "Corner should be outside the sphere, got {}",
        values[corner_idx]
    );
}

#[test]
fn test_bake_single_brick_no_nan() {
    let (ctx, pipeline, layout, config) = setup();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0)
        .expect("bake_brick");

    assert!(
        values.iter().all(|v| v.is_finite()),
        "All values should be finite (no NaN/Inf)"
    );
}

#[test]
fn test_bake_all_bricks_single() {
    let (ctx, pipeline, layout, config) = setup();
    // 64x64x64 with brick_size=64 → 1 brick
    let results = bake_all_bricks(&ctx, &pipeline, &layout, &config)
        .expect("bake_all_bricks");
    assert_eq!(results.len(), 1);
    assert!(!results[0].is_background, "Sphere brick should be active");
}

#[test]
fn test_bake_sphere_surface_values() {
    let (ctx, pipeline, layout, config) = setup();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0)
        .expect("bake_brick");

    // At a point on the sphere surface: (32 + 25.6, 32, 32) → voxel (57, 32, 32)
    // voxel center = (57.5, 32.5, 32.5), distance ≈ length((25.5, 0.5, 0.5)) - 25.6
    // ≈ 25.51 - 25.6 ≈ -0.09 (just inside)
    let near_surface_idx = 57 + 64 * (32 + 64 * 32);
    let d = values[near_surface_idx];
    assert!(
        d.abs() < 2.0,
        "Near surface, distance should be close to 0, got {d}"
    );
}
