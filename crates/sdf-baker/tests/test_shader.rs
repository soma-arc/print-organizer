/// Integration tests for external shader loading (WGSL and GLSL).
use std::fs;

use sdf_baker::compute::{bake_brick, create_compute_pipeline};
use sdf_baker::gpu::init_gpu;
use sdf_baker::shader_compose::{ShaderLang, compose_shader, load_shader};
use sdf_baker::types::BakeConfig;

fn default_config() -> BakeConfig {
    BakeConfig::new(
        [0.0, 0.0, 0.0],
        [64.0, 64.0, 64.0],
        1.0,
        64,
        3,
        0.0,
        0.0,
        "f32".to_string(),
    )
}

#[test]
fn test_external_wgsl_shader_bake() {
    let dir = tempfile::tempdir().unwrap();
    let shader_path = dir.path().join("sphere.wgsl");
    fs::write(
        &shader_path,
        r#"
fn sdf(p: vec3<f32>) -> f32 {
    return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
}
"#,
    )
    .unwrap();

    let (lang, code) = load_shader(&shader_path).unwrap();
    assert_eq!(lang, ShaderLang::Wgsl);

    let composed = compose_shader(lang, &code).unwrap();
    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();
    let config = default_config();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0).unwrap();

    assert_eq!(values.len(), 64 * 64 * 64);

    // Center should be inside sphere
    let center = 32 + 64 * (32 + 64 * 32);
    assert!(
        values[center] < 0.0,
        "Center should be negative, got {}",
        values[center]
    );

    // Corner should be outside
    assert!(
        values[0] > 0.0,
        "Corner should be positive, got {}",
        values[0]
    );
}

#[test]
fn test_external_glsl_shader_bake() {
    let dir = tempfile::tempdir().unwrap();
    let shader_path = dir.path().join("sphere.glsl");
    fs::write(
        &shader_path,
        "float sdf(vec3 p) { return length(p - vec3(32.0)) - 25.6; }",
    )
    .unwrap();

    let (lang, code) = load_shader(&shader_path).unwrap();
    assert_eq!(lang, ShaderLang::Glsl);

    let composed = compose_shader(lang, &code).unwrap();
    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();
    let config = default_config();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0).unwrap();

    assert_eq!(values.len(), 64 * 64 * 64);

    // Center should be inside sphere
    let center = 32 + 64 * (32 + 64 * 32);
    assert!(
        values[center] < 0.0,
        "Center should be negative, got {}",
        values[center]
    );

    // Corner should be outside
    assert!(
        values[0] > 0.0,
        "Corner should be positive, got {}",
        values[0]
    );
}

#[test]
fn test_glsl_and_wgsl_produce_same_values() {
    let dir = tempfile::tempdir().unwrap();

    // WGSL sphere
    let wgsl_path = dir.path().join("sphere.wgsl");
    fs::write(
        &wgsl_path,
        r#"
fn sdf(p: vec3<f32>) -> f32 {
    return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
}
"#,
    )
    .unwrap();

    // GLSL sphere
    let glsl_path = dir.path().join("sphere.glsl");
    fs::write(
        &glsl_path,
        "float sdf(vec3 p) { return length(p - vec3(32.0)) - 25.6; }",
    )
    .unwrap();

    let ctx = init_gpu().unwrap();
    let config = default_config();

    // Bake with WGSL
    let (wlang, wcode) = load_shader(&wgsl_path).unwrap();
    let wcomposed = compose_shader(wlang, &wcode).unwrap();
    let (wp, wl) =
        create_compute_pipeline(&ctx.device, &wcomposed.wgsl_source, &wcomposed.entry_point)
            .unwrap();
    let wgsl_values = bake_brick(&ctx, &wp, &wl, &config, 0, 0, 0).unwrap();

    // Bake with GLSL
    let (glang, gcode) = load_shader(&glsl_path).unwrap();
    let gcomposed = compose_shader(glang, &gcode).unwrap();
    let (gp, gl) =
        create_compute_pipeline(&ctx.device, &gcomposed.wgsl_source, &gcomposed.entry_point)
            .unwrap();
    let glsl_values = bake_brick(&ctx, &gp, &gl, &config, 0, 0, 0).unwrap();

    assert_eq!(wgsl_values.len(), glsl_values.len());

    // Values should be very close (floating point equivalence)
    let max_diff: f32 = wgsl_values
        .iter()
        .zip(glsl_values.iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);

    assert!(
        max_diff < 0.01,
        "WGSL and GLSL should produce nearly identical values, max diff = {max_diff}"
    );
}

#[test]
fn test_external_wgsl_custom_sdf() {
    // Test a different SDF - a box
    let dir = tempfile::tempdir().unwrap();
    let shader_path = dir.path().join("box.wgsl");
    fs::write(
        &shader_path,
        r#"
fn sdf(p: vec3<f32>) -> f32 {
    let center = vec3<f32>(32.0, 32.0, 32.0);
    let half_size = vec3<f32>(20.0, 20.0, 20.0);
    let d = abs(p - center) - half_size;
    return length(max(d, vec3<f32>(0.0))) + min(max(d.x, max(d.y, d.z)), 0.0);
}
"#,
    )
    .unwrap();

    let (lang, code) = load_shader(&shader_path).unwrap();
    let composed = compose_shader(lang, &code).unwrap();
    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();
    let config = default_config();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0).unwrap();

    // Center should be inside box
    let center = 32 + 64 * (32 + 64 * 32);
    assert!(
        values[center] < 0.0,
        "Box center should be negative, got {}",
        values[center]
    );

    // Corner (0,0,0) should be outside
    assert!(
        values[0] > 0.0,
        "Corner should be positive, got {}",
        values[0]
    );
}

#[test]
fn test_external_glsl_custom_sdf() {
    // Test a different SDF - a box in GLSL
    let dir = tempfile::tempdir().unwrap();
    let shader_path = dir.path().join("box.comp");
    fs::write(
        &shader_path,
        r#"float sdf(vec3 p) {
    vec3 center = vec3(32.0, 32.0, 32.0);
    vec3 half_size = vec3(20.0, 20.0, 20.0);
    vec3 d = abs(p - center) - half_size;
    return length(max(d, vec3(0.0))) + min(max(d.x, max(d.y, d.z)), 0.0);
}"#,
    )
    .unwrap();

    let (lang, code) = load_shader(&shader_path).unwrap();
    assert_eq!(lang, ShaderLang::Glsl);
    let composed = compose_shader(lang, &code).unwrap();
    let ctx = init_gpu().unwrap();
    let (pipeline, layout) =
        create_compute_pipeline(&ctx.device, &composed.wgsl_source, &composed.entry_point).unwrap();
    let config = default_config();
    let values = bake_brick(&ctx, &pipeline, &layout, &config, 0, 0, 0).unwrap();

    // Center should be inside box
    let center = 32 + 64 * (32 + 64 * 32);
    assert!(
        values[center] < 0.0,
        "Box center should be negative, got {}",
        values[center]
    );
}
