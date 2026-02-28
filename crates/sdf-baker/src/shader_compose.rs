/// Shader composition: merge user SDF function with the compute dispatch template.

const COMPUTE_TEMPLATE_WGSL: &str = include_str!("../shaders/compute_template.wgsl");

const PLACEHOLDER: &str = "{{USER_SDF}}";

/// Built-in sphere SDF used when no user shader is provided.
pub const BUILTIN_SPHERE_SDF: &str = r#"
fn sdf(p: vec3<f32>) -> f32 {
    return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
}
"#;

/// Compose a complete WGSL compute shader by inserting `user_sdf_code` into the template.
///
/// The user code must define `fn sdf(p: vec3<f32>) -> f32`.
pub fn compose_wgsl(user_sdf_code: &str) -> String {
    COMPUTE_TEMPLATE_WGSL.replace(PLACEHOLDER, user_sdf_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compose_wgsl_contains_user_code() {
        let result = compose_wgsl(BUILTIN_SPHERE_SDF);
        assert!(result.contains("fn sdf(p: vec3<f32>) -> f32"));
        assert!(result.contains("fn cs_main"));
        assert!(!result.contains(PLACEHOLDER));
    }

    #[test]
    fn test_compose_wgsl_placeholder_replaced() {
        let result = compose_wgsl("fn sdf(p: vec3<f32>) -> f32 { return 1.0; }");
        assert!(!result.contains(PLACEHOLDER));
        assert!(result.contains("return 1.0;"));
    }

    #[test]
    fn test_compose_wgsl_validates_with_naga() {
        // Verify the composed shader can be parsed by naga (used internally by wgpu).
        let source = compose_wgsl(BUILTIN_SPHERE_SDF);
        let module = naga::front::wgsl::parse_str(&source);
        assert!(module.is_ok(), "naga parse failed: {:?}", module.err());
    }
}
