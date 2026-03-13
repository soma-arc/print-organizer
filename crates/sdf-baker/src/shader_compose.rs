/// Shader composition: merge user SDF function with the compute dispatch template.
///
/// Supports both WGSL and GLSL shader languages. GLSL shaders are converted
/// to WGSL via naga before being passed to wgpu.
use std::path::Path;

use anyhow::{Context, Result, bail};

/// A single shader diagnostic with optional source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderDiagnostic {
    /// 1-based line number in the user's source (if available).
    pub line: Option<u32>,
    /// 1-based column (if available).
    pub column: Option<u32>,
    /// Human-readable error message.
    pub message: String,
}

impl std::fmt::Display for ShaderDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.line {
            Some(l) => write!(f, "L{l}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// Error type for shader compilation failures with structured diagnostics.
#[derive(Debug, Clone)]
pub struct ShaderDiagnostics {
    pub diagnostics: Vec<ShaderDiagnostic>,
}

impl std::fmt::Display for ShaderDiagnostics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, d) in self.diagnostics.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ShaderDiagnostics {}

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
    naga::front::wgsl::parse_str(source).map_err(|e| {
        let diags = wgsl_parse_error_to_diagnostics(&e, source);
        anyhow::Error::new(diags)
    })?;
    Ok(())
}

/// Extract structured diagnostics from a WGSL parse error.
fn wgsl_parse_error_to_diagnostics(
    err: &naga::front::wgsl::ParseError,
    source: &str,
) -> ShaderDiagnostics {
    let mut diagnostics = Vec::new();
    let mut first = true;
    for (span, _label) in err.labels() {
        if first {
            let loc = span.location(source);
            diagnostics.push(ShaderDiagnostic {
                line: Some(loc.line_number),
                column: Some(loc.line_position),
                message: err.message().to_string(),
            });
            first = false;
        }
    }
    if first {
        // No spans available
        diagnostics.push(ShaderDiagnostic {
            line: None,
            column: None,
            message: err.message().to_string(),
        });
    }
    ShaderDiagnostics { diagnostics }
}

/// Convert a complete GLSL compute shader to WGSL using naga.
pub fn glsl_to_wgsl(glsl_source: &str) -> Result<String> {
    glsl_to_wgsl_with_offset(glsl_source, 0)
}

/// Convert a complete GLSL compute shader to WGSL using naga,
/// subtracting `line_offset` from reported line numbers to map back
/// to the user's original source.
pub fn glsl_to_wgsl_with_offset(glsl_source: &str, line_offset: u32) -> Result<String> {
    use naga::back::wgsl;
    use naga::front::glsl::{Frontend, Options};
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    // Parse GLSL
    let mut frontend = Frontend::default();
    let options = Options::from(naga::ShaderStage::Compute);
    let module = frontend
        .parse(&options, glsl_source)
        .map_err(|parse_errors| {
            let diagnostics: Vec<ShaderDiagnostic> = parse_errors
                .errors
                .iter()
                .map(|e| {
                    let loc = e.meta.location(glsl_source);
                    let user_line = loc.line_number.saturating_sub(line_offset);
                    ShaderDiagnostic {
                        line: if user_line > 0 { Some(user_line) } else { None },
                        column: Some(loc.line_position),
                        message: format!("{}", e.kind),
                    }
                })
                .collect();
            anyhow::Error::new(ShaderDiagnostics { diagnostics })
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

    #[test]
    fn test_glsl_diagnostics_have_line_numbers() {
        // Line 1: #version 450
        // Line 2: layout...
        // Line 3: buffer...
        // Line 4: user code starts → error here
        let glsl = "#version 450\nlayout(local_size_x=1) in;\nlayout(std430, set=0, binding=0) buffer O { float d[]; } o;\nfloat sdf(vec3 p) { return UNDEFINED_SYMBOL; }\nvoid main() { o.d[0] = sdf(vec3(0.0)); }\n";
        let result = glsl_to_wgsl(glsl);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let diags = err.downcast_ref::<ShaderDiagnostics>().unwrap();
        assert!(!diags.diagnostics.is_empty(), "Should have diagnostics");
        // The error should reference line 4 (where UNDEFINED_SYMBOL is)
        let first = &diags.diagnostics[0];
        assert!(first.line.is_some(), "Should have line number");
    }

    #[test]
    fn test_glsl_diagnostics_with_offset_correction() {
        // Simulate the preview wrapper: 3 preamble lines
        let glsl = "#version 450\nlayout(local_size_x=1) in;\nlayout(std430, set=0, binding=0) buffer O { float d[]; } o;\nfloat sdf(vec3 p) { return UNDEFINED_SYMBOL; }\nvoid main() { o.d[0] = sdf(vec3(0.0)); }\n";
        let result = glsl_to_wgsl_with_offset(glsl, 3);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let diags = err.downcast_ref::<ShaderDiagnostics>().unwrap();
        let first = &diags.diagnostics[0];
        // Line 4 in full source minus 3 offset = line 1 in user source
        assert!(first.line.is_some());
        assert!(
            first.line.unwrap() <= 2,
            "With offset=3, user line should be small; got {}",
            first.line.unwrap()
        );
    }

    #[test]
    fn test_wgsl_diagnostics_have_line_numbers() {
        let bad_wgsl = "fn sdf(p: vec3<f32>) -> f32 {\n    return MISSING;\n}\n";
        let result = validate_wgsl(bad_wgsl);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let diags = err.downcast_ref::<ShaderDiagnostics>().unwrap();
        assert!(!diags.diagnostics.is_empty());
        let first = &diags.diagnostics[0];
        assert!(first.line.is_some(), "WGSL error should have line number");
    }

    #[test]
    fn test_shader_diagnostic_display() {
        let d = ShaderDiagnostic {
            line: Some(5),
            column: Some(10),
            message: "unknown identifier".to_string(),
        };
        assert_eq!(d.to_string(), "L5: unknown identifier");

        let d_no_line = ShaderDiagnostic {
            line: None,
            column: None,
            message: "general error".to_string(),
        };
        assert_eq!(d_no_line.to_string(), "general error");
    }
}
