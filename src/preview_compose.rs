/// Preview shader composition: merge user SDF function with the raymarch template.
///
/// Reuses `sdf_baker::shader_compose::{load_shader, glsl_to_wgsl, validate_wgsl}`.
use anyhow::Result;
use sdf_baker::shader_compose::{ShaderLang, validate_wgsl};

const RAYMARCH_TEMPLATE_WGSL: &str = include_str!("shaders/raymarch_template.wgsl");
const RAYMARCH_TEMPLATE_GLSL: &str = include_str!("shaders/raymarch_template.glsl");

const PLACEHOLDER: &str = "{{USER_SDF}}";

/// Compose a preview (raymarch) shader from user SDF code.
///
/// Returns a complete WGSL source ready for `device.create_shader_module()`.
pub fn compose_preview(lang: ShaderLang, user_sdf_code: &str) -> Result<String> {
    match lang {
        ShaderLang::Wgsl => {
            let wgsl = RAYMARCH_TEMPLATE_WGSL.replace(PLACEHOLDER, user_sdf_code);
            validate_wgsl(&wgsl)?;
            Ok(wgsl)
        }
        ShaderLang::Glsl => {
            let glsl = RAYMARCH_TEMPLATE_GLSL.replace(PLACEHOLDER, user_sdf_code);
            let wgsl = glsl_fragment_to_wgsl(&glsl)?;
            Ok(wgsl)
        }
    }
}

/// Convert a complete GLSL fragment shader to WGSL using naga.
fn glsl_fragment_to_wgsl(glsl_source: &str) -> Result<String> {
    use naga::back::wgsl;
    use naga::front::glsl::{Frontend, Options};
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    let mut frontend = Frontend::default();
    let options = Options::from(naga::ShaderStage::Fragment);
    let module = frontend
        .parse(&options, glsl_source)
        .map_err(|parse_errors| {
            let msgs: Vec<String> = parse_errors.errors.iter().map(|e| format!("{e}")).collect();
            anyhow::anyhow!("GLSL parse errors:\n{}", msgs.join("\n"))
        })?;

    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| anyhow::anyhow!("GLSL shader validation error: {e}"))?;

    let wgsl_source = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| anyhow::anyhow!("GLSL→WGSL conversion failed: {e}"))?;

    Ok(wgsl_source)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPHERE_WGSL: &str = r#"
fn sdf(p: vec3<f32>) -> f32 {
    return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
}
"#;

    const SPHERE_GLSL: &str = r#"
float sdf(vec3 p) {
    return length(p - vec3(32.0, 32.0, 32.0)) - 25.6;
}
"#;

    #[test]
    fn test_compose_wgsl_preview() {
        let result = compose_preview(ShaderLang::Wgsl, SPHERE_WGSL);
        assert!(
            result.is_ok(),
            "WGSL preview composition failed: {result:?}"
        );
        let wgsl = result.unwrap();
        assert!(wgsl.contains("fn sdf("));
        assert!(wgsl.contains("fn vs_main("));
        assert!(wgsl.contains("fn fs_main("));
    }

    #[test]
    fn test_compose_glsl_preview() {
        let result = compose_preview(ShaderLang::Glsl, SPHERE_GLSL);
        assert!(
            result.is_ok(),
            "GLSL preview composition failed: {result:?}"
        );
    }

    #[test]
    fn test_placeholder_replaced() {
        let wgsl = compose_preview(ShaderLang::Wgsl, SPHERE_WGSL).unwrap();
        assert!(!wgsl.contains(PLACEHOLDER));
    }
}
