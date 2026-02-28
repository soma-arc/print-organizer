/// Shader composition: merge user SDF function with the compute dispatch template.
///
/// Supports both WGSL and GLSL shader languages. GLSL shaders are converted
/// to WGSL via naga before being passed to wgpu.
use std::path::Path;

use anyhow::{Context, Result, bail};

const COMPUTE_TEMPLATE_WGSL: &str = include_str!("../shaders/compute_template.wgsl");
const COMPUTE_TEMPLATE_GLSL: &str = include_str!("../shaders/compute_template.glsl");

const PLACEHOLDER: &str = "{{USER_SDF}}";

/// Built-in sphere SDF used when no user shader is provided (WGSL).
pub const BUILTIN_SPHERE_SDF: &str = r#"
fn sdf(p: vec3<f32>) -> f32 {
    return length(p - vec3<f32>(32.0, 32.0, 32.0)) - 25.6;
}
"#;

/// Shader language detected from file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderLang {
    Wgsl,
    Glsl,
}

/// Load a shader file, detect its language from the extension, and validate
/// that it defines an `sdf()` function.
pub fn load_shader(path: &Path) -> Result<(ShaderLang, String)> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    let lang = match ext.as_str() {
        "wgsl" => ShaderLang::Wgsl,
        "glsl" | "comp" | "frag" => ShaderLang::Glsl,
        _ => bail!("Unsupported shader extension '.{ext}'. Use .wgsl or .glsl/.comp"),
    };

    let code = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read shader file: {}", path.display()))?;

    validate_sdf_function(lang, &code)?;

    Ok((lang, code))
}

/// Validate that the shader code contains an `sdf()` function definition.
fn validate_sdf_function(lang: ShaderLang, code: &str) -> Result<()> {
    let has_sdf = match lang {
        ShaderLang::Wgsl => code.contains("fn sdf("),
        ShaderLang::Glsl => code.contains("float sdf("),
    };
    if !has_sdf {
        let expected = match lang {
            ShaderLang::Wgsl => "fn sdf(p: vec3<f32>) -> f32",
            ShaderLang::Glsl => "float sdf(vec3 p)",
        };
        bail!("Shader must define an sdf function: `{expected}`");
    }
    Ok(())
}

/// Compose a complete WGSL compute shader by inserting `user_sdf_code` into the template.
///
/// The user code must define `fn sdf(p: vec3<f32>) -> f32`.
pub fn compose_wgsl(user_sdf_code: &str) -> String {
    COMPUTE_TEMPLATE_WGSL.replace(PLACEHOLDER, user_sdf_code)
}

/// Compose a complete GLSL compute shader by inserting `user_sdf_code` into the template.
///
/// The user code must define `float sdf(vec3 p)`.
pub fn compose_glsl(user_sdf_code: &str) -> String {
    COMPUTE_TEMPLATE_GLSL.replace(PLACEHOLDER, user_sdf_code)
}

/// Compose a shader from user code and the appropriate template,
/// returning WGSL source ready for wgpu.
///
/// Result of shader composition: WGSL source ready for wgpu, plus entry point name.
#[derive(Debug, Clone)]
pub struct ComposedShader {
    /// Complete WGSL source code.
    pub wgsl_source: String,
    /// Compute shader entry point name ("cs_main" for WGSL, "main" for GLSL→WGSL).
    pub entry_point: String,
}

/// Compose a shader from user code and the appropriate template,
/// returning WGSL source ready for wgpu.
///
/// For WGSL input, returns the composed WGSL directly (after naga validation).
/// For GLSL input, composes with the GLSL template, then converts to WGSL via naga.
pub fn compose_shader(lang: ShaderLang, user_sdf_code: &str) -> Result<ComposedShader> {
    match lang {
        ShaderLang::Wgsl => {
            let wgsl = compose_wgsl(user_sdf_code);
            validate_wgsl(&wgsl)?;
            Ok(ComposedShader {
                wgsl_source: wgsl,
                entry_point: "cs_main".to_string(),
            })
        }
        ShaderLang::Glsl => {
            let glsl = compose_glsl(user_sdf_code);
            let wgsl = glsl_to_wgsl(&glsl)?;
            Ok(ComposedShader {
                wgsl_source: wgsl,
                entry_point: "main".to_string(),
            })
        }
    }
}

/// Validate a WGSL shader source using naga's parser.
pub fn validate_wgsl(source: &str) -> Result<()> {
    naga::front::wgsl::parse_str(source)
        .map_err(|e| anyhow::anyhow!("WGSL validation failed: {e}"))?;
    Ok(())
}

/// Convert a complete GLSL compute shader to WGSL using naga.
pub fn glsl_to_wgsl(glsl_source: &str) -> Result<String> {
    use naga::back::wgsl;
    use naga::front::glsl::{Frontend, Options};
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    // Parse GLSL
    let mut frontend = Frontend::default();
    let options = Options::from(naga::ShaderStage::Compute);
    let module = frontend
        .parse(&options, glsl_source)
        .map_err(|parse_errors| {
            let msgs: Vec<String> = parse_errors.errors.iter().map(|e| format!("{e}")).collect();
            anyhow::anyhow!("GLSL parse errors:\n{}", msgs.join("\n"))
        })?;

    // Validate
    let info = Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(&module)
        .map_err(|e| anyhow::anyhow!("GLSL shader validation error: {e}"))?;

    // Convert to WGSL
    let wgsl_source = wgsl::write_string(&module, &info, wgsl::WriterFlags::empty())
        .map_err(|e| anyhow::anyhow!("GLSL→WGSL conversion failed: {e}"))?;

    Ok(wgsl_source)
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
        let source = compose_wgsl(BUILTIN_SPHERE_SDF);
        assert!(validate_wgsl(&source).is_ok());
    }

    #[test]
    fn test_compose_shader_wgsl() {
        let result = compose_shader(ShaderLang::Wgsl, BUILTIN_SPHERE_SDF);
        assert!(result.is_ok());
        let composed = result.unwrap();
        assert!(composed.wgsl_source.contains("cs_main"));
        assert_eq!(composed.entry_point, "cs_main");
    }

    #[test]
    fn test_compose_glsl_contains_user_code() {
        let user = "float sdf(vec3 p) { return length(p - vec3(32.0)) - 25.6; }";
        let result = compose_glsl(user);
        assert!(result.contains("float sdf(vec3 p)"));
        assert!(result.contains("void main()"));
        assert!(!result.contains(PLACEHOLDER));
    }

    #[test]
    fn test_compose_shader_glsl_to_wgsl() {
        let user = "float sdf(vec3 p) { return length(p - vec3(32.0)) - 25.6; }";
        let result = compose_shader(ShaderLang::Glsl, user);
        assert!(result.is_ok(), "GLSL→WGSL failed: {:?}", result.err());
        let composed = result.unwrap();
        assert_eq!(composed.entry_point, "main");
    }

    #[test]
    fn test_load_shader_detects_wgsl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.wgsl");
        std::fs::write(&path, "fn sdf(p: vec3<f32>) -> f32 { return 1.0; }").unwrap();
        let (lang, _code) = load_shader(&path).unwrap();
        assert_eq!(lang, ShaderLang::Wgsl);
    }

    #[test]
    fn test_load_shader_detects_glsl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.glsl");
        std::fs::write(&path, "float sdf(vec3 p) { return 1.0; }").unwrap();
        let (lang, _code) = load_shader(&path).unwrap();
        assert_eq!(lang, ShaderLang::Glsl);
    }

    #[test]
    fn test_load_shader_detects_comp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.comp");
        std::fs::write(&path, "float sdf(vec3 p) { return 1.0; }").unwrap();
        let (lang, _code) = load_shader(&path).unwrap();
        assert_eq!(lang, ShaderLang::Glsl);
    }

    #[test]
    fn test_load_shader_rejects_missing_sdf_wgsl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.wgsl");
        std::fs::write(&path, "fn not_sdf(p: vec3<f32>) -> f32 { return 1.0; }").unwrap();
        let result = load_shader(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_shader_rejects_missing_sdf_glsl() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.glsl");
        std::fs::write(&path, "float not_sdf(vec3 p) { return 1.0; }").unwrap();
        let result = load_shader(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_shader_rejects_unknown_extension() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.hlsl");
        std::fs::write(&path, "float sdf(float3 p) { return 1.0; }").unwrap();
        let result = load_shader(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_glsl_to_wgsl_sphere() {
        let glsl = compose_glsl("float sdf(vec3 p) { return length(p - vec3(32.0)) - 25.6; }");
        let wgsl = glsl_to_wgsl(&glsl);
        assert!(wgsl.is_ok(), "GLSL→WGSL failed: {:?}", wgsl.err());
    }

    #[test]
    fn test_glsl_to_wgsl_invalid() {
        let result = glsl_to_wgsl("this is not valid GLSL");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_wgsl_valid() {
        let source = compose_wgsl(BUILTIN_SPHERE_SDF);
        assert!(validate_wgsl(&source).is_ok());
    }

    #[test]
    fn test_validate_wgsl_invalid() {
        let result = validate_wgsl("this is not valid WGSL");
        assert!(result.is_err());
    }
}
