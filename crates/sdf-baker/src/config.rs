/// Configuration file loading and merging with CLI arguments.
///
/// The config file is JSON with all fields optional. CLI arguments
/// override config file values, which override defaults.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::cli::Cli;
use crate::types::BakeConfig;

/// Top-level config file structure. All fields are optional.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct ConfigFile {
    /// Schema version. None or absent = v1. 2 = v2 (with presets).
    pub version: Option<u32>,

    /// Path to SDF shader file (resolved relative to config file).
    pub shader: Option<String>,
    /// Output directory (resolved relative to config file directory).
    pub out: Option<String>,

    /// Grid parameters.
    pub grid: GridConfig,
    /// Bake parameters.
    pub bake: BakeParams,
    /// Mesh parameters.
    pub mesh: MeshParams,
    /// Genmesh invocation parameters.
    pub genmesh: GenmeshConfig,

    /// Preset overrides (v2).
    pub presets: Option<Vec<PresetEntry>>,
}

/// A single preset entry. All fields except `name` are optional;
/// only the fields present will override the base config.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct PresetEntry {
    /// Preset display name (required).
    pub name: String,

    pub shader: Option<String>,
    pub out: Option<String>,
    pub grid: Option<GridConfig>,
    pub bake: Option<BakeParams>,
    pub mesh: Option<MeshParams>,
    pub genmesh: Option<GenmeshConfig>,
}

/// Merge a preset's overrides onto a base config.
///
/// For each `Option` field: if the preset has `Some`, it replaces the base;
/// otherwise the base value is inherited. Sub-structs are merged at field level.
pub fn merge_preset(base: &ConfigFile, preset: &PresetEntry) -> ConfigFile {
    ConfigFile {
        version: base.version,
        shader: preset.shader.clone().or_else(|| base.shader.clone()),
        out: preset.out.clone().or_else(|| base.out.clone()),
        grid: match &preset.grid {
            Some(pg) => GridConfig {
                aabb_min: pg.aabb_min.or(base.grid.aabb_min),
                aabb_size: pg.aabb_size.or(base.grid.aabb_size),
                voxel_size: pg.voxel_size.or(base.grid.voxel_size),
                brick_size: pg.brick_size.or(base.grid.brick_size),
            },
            None => base.grid.clone(),
        },
        bake: match &preset.bake {
            Some(pb) => BakeParams {
                half_width: pb.half_width.or(base.bake.half_width),
                dtype: pb.dtype.clone().or_else(|| base.bake.dtype.clone()),
            },
            None => base.bake.clone(),
        },
        mesh: match &preset.mesh {
            Some(pm) => MeshParams {
                iso: pm.iso.or(base.mesh.iso),
                adaptivity: pm.adaptivity.or(base.mesh.adaptivity),
            },
            None => base.mesh.clone(),
        },
        genmesh: match &preset.genmesh {
            Some(pg) => GenmeshConfig {
                path: pg.path.clone().or_else(|| base.genmesh.path.clone()),
                write_vdb: pg.write_vdb.or(base.genmesh.write_vdb),
                skip: pg.skip.or(base.genmesh.skip),
            },
            None => base.genmesh.clone(),
        },
        presets: None, // presets are not nested
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct GridConfig {
    pub aabb_min: Option<[f32; 3]>,
    pub aabb_size: Option<[f32; 3]>,
    pub voxel_size: Option<f32>,
    pub brick_size: Option<u32>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct BakeParams {
    pub half_width: Option<u32>,
    pub dtype: Option<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct MeshParams {
    pub iso: Option<f32>,
    pub adaptivity: Option<f32>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct GenmeshConfig {
    pub path: Option<String>,
    pub write_vdb: Option<bool>,
    pub skip: Option<bool>,
}

/// Fully resolved configuration after merging config file + CLI.
#[derive(Debug)]
pub struct ResolvedConfig {
    pub shader: Option<PathBuf>,
    pub out: PathBuf,
    pub bake_config: BakeConfig,
    pub genmesh_path: Option<PathBuf>,
    pub skip_genmesh: bool,
    pub write_vdb: bool,
    pub force: bool,
    pub log_level: String,
}

/// Load a config file from disk.
pub fn load_config(path: &Path) -> Result<ConfigFile> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let config: ConfigFile = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
    Ok(config)
}

/// Resolve genmesh executable path using a 4-tier search:
///
/// 1. Explicit path (from config or CLI `--genmesh-path`)
/// 2. `PRINT_ORGANIZER_GENMESH` environment variable
/// 3. Same directory as the current executable (`exe_dir/genmesh`)
/// 4. Bare name `"genmesh"` (relies on PATH)
pub fn resolve_genmesh_path(explicit: Option<PathBuf>) -> PathBuf {
    // Tier 1: explicit path
    if let Some(p) = explicit {
        return p;
    }

    // Tier 2: environment variable
    if let Ok(env_path) = std::env::var("PRINT_ORGANIZER_GENMESH") {
        if !env_path.is_empty() {
            return PathBuf::from(env_path);
        }
    }

    // Tier 3: exe-adjacent
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let candidate = if cfg!(windows) {
                exe_dir.join("genmesh.exe")
            } else {
                exe_dir.join("genmesh")
            };
            if candidate.exists() {
                return candidate;
            }
        }
    }

    // Tier 4: PATH fallback
    PathBuf::from("genmesh")
}

/// Merge CLI arguments with an optional config file into a ResolvedConfig.
///
/// Priority: CLI explicit args > config file > CLI defaults.
///
/// The `cli_matches` is used to detect which CLI args were explicitly set
/// by the user (vs. just having default values).
pub fn resolve_config(cli: &Cli, config_path: Option<&Path>) -> Result<ResolvedConfig> {
    let config = if let Some(path) = config_path {
        Some((load_config(path)?, path.to_path_buf()))
    } else {
        None
    };

    let (cfg, cfg_dir) = match &config {
        Some((cfg, path)) => {
            let dir = path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
            (Some(cfg), Some(dir))
        }
        None => (None, None),
    };

    // Resolve shader path:
    // CLI --shader takes priority, then config file shader (relative to config dir).
    let shader = if cli.shader.is_some() {
        cli.shader.clone()
    } else if let Some(ref shader_str) = cfg.and_then(|c| c.shader.as_ref()) {
        let base = cfg_dir.as_deref().unwrap_or_else(|| Path::new("."));
        Some(base.join(shader_str))
    } else {
        None
    };

    // Resolve output directory: CLI --out takes priority (CWD-relative),
    // then config file out (config-relative).
    let out = if let Some(ref cli_out) = cli.out {
        cli_out.clone()
    } else if let Some(ref out_str) = cfg.and_then(|c| c.out.as_ref()) {
        let base = cfg_dir.as_deref().unwrap_or_else(|| Path::new("."));
        base.join(out_str)
    } else {
        anyhow::bail!("Output directory must be specified via --out or config file 'out' field");
    };

    // Grid params: CLI > config > defaults
    let aabb_min = cfg.and_then(|c| c.grid.aabb_min).unwrap_or(cli.aabb_min);
    let aabb_size = cfg.and_then(|c| c.grid.aabb_size).unwrap_or(cli.aabb_size);
    let voxel_size = cfg
        .and_then(|c| c.grid.voxel_size)
        .unwrap_or(cli.voxel_size);
    let brick_size = cfg
        .and_then(|c| c.grid.brick_size)
        .unwrap_or(cli.brick_size);

    // Bake params
    let half_width = cfg
        .and_then(|c| c.bake.half_width)
        .unwrap_or(cli.half_width);
    let dtype = cfg
        .and_then(|c| c.bake.dtype.clone())
        .unwrap_or_else(|| cli.dtype.clone());

    // Mesh params
    let iso = cfg.and_then(|c| c.mesh.iso).unwrap_or(cli.iso);
    let adaptivity = cfg
        .and_then(|c| c.mesh.adaptivity)
        .unwrap_or(cli.adaptivity);

    // Genmesh params
    let genmesh_path = if cli.genmesh_path.is_some() {
        cli.genmesh_path.clone()
    } else if let Some(ref path_str) = cfg.and_then(|c| c.genmesh.path.as_ref()) {
        let base = cfg_dir.as_deref().unwrap_or_else(|| Path::new("."));
        Some(base.join(path_str))
    } else {
        None
    };

    let skip_genmesh = cfg.and_then(|c| c.genmesh.skip).unwrap_or(cli.skip_genmesh);
    let write_vdb = cfg
        .and_then(|c| c.genmesh.write_vdb)
        .unwrap_or(cli.write_vdb);

    // Validate brick_size
    if brick_size != 32 && brick_size != 64 && brick_size != 128 {
        anyhow::bail!("brick_size must be 32, 64, or 128, got {brick_size}");
    }

    // Validate adaptivity
    if !(0.0..=1.0).contains(&adaptivity) {
        anyhow::bail!("adaptivity must be 0.0..=1.0, got {adaptivity}");
    }

    // Validate dtype
    if dtype != "f32" && dtype != "f16" {
        anyhow::bail!("dtype must be 'f32' or 'f16', got '{dtype}'");
    }

    let bake_config = BakeConfig::new(
        aabb_min, aabb_size, voxel_size, brick_size, half_width, iso, adaptivity, dtype,
    );

    Ok(ResolvedConfig {
        shader,
        out,
        bake_config,
        genmesh_path,
        skip_genmesh,
        write_vdb,
        force: cli.force,
        log_level: cli.log_level.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn default_cli(out: &str) -> Cli {
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
    fn test_load_config_full() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("test.json");
        let mut f = std::fs::File::create(&config_path).unwrap();
        write!(
            f,
            r#"{{
                "shader": "my_sdf.wgsl",
                "out": "output",
                "grid": {{
                    "aabb_min": [-10, -10, -10],
                    "aabb_size": [128, 128, 128],
                    "voxel_size": 0.5,
                    "brick_size": 32
                }},
                "bake": {{
                    "half_width": 5,
                    "dtype": "f16"
                }},
                "mesh": {{
                    "iso": 0.1,
                    "adaptivity": 0.3
                }},
                "genmesh": {{
                    "path": "genmesh.exe",
                    "write_vdb": true,
                    "skip": false
                }}
            }}"#
        )
        .unwrap();

        let cfg = load_config(&config_path).unwrap();
        assert_eq!(cfg.shader.as_deref(), Some("my_sdf.wgsl"));
        assert_eq!(cfg.out.as_deref(), Some("output"));
        assert_eq!(cfg.grid.aabb_min, Some([-10.0, -10.0, -10.0]));
        assert_eq!(cfg.grid.aabb_size, Some([128.0, 128.0, 128.0]));
        assert_eq!(cfg.grid.voxel_size, Some(0.5));
        assert_eq!(cfg.grid.brick_size, Some(32));
        assert_eq!(cfg.bake.half_width, Some(5));
        assert_eq!(cfg.bake.dtype.as_deref(), Some("f16"));
        assert_eq!(cfg.mesh.iso, Some(0.1));
        assert_eq!(cfg.mesh.adaptivity, Some(0.3));
        assert_eq!(cfg.genmesh.path.as_deref(), Some("genmesh.exe"));
        assert_eq!(cfg.genmesh.write_vdb, Some(true));
        assert_eq!(cfg.genmesh.skip, Some(false));
    }

    #[test]
    fn test_load_config_empty() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("empty.json");
        std::fs::write(&config_path, "{}").unwrap();

        let cfg = load_config(&config_path).unwrap();
        assert!(cfg.shader.is_none());
        assert!(cfg.out.is_none());
        assert!(cfg.grid.aabb_min.is_none());
    }

    #[test]
    fn test_load_config_partial() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("partial.json");
        std::fs::write(
            &config_path,
            r#"{ "grid": { "aabb_size": [256, 256, 256] } }"#,
        )
        .unwrap();

        let cfg = load_config(&config_path).unwrap();
        assert!(cfg.shader.is_none());
        assert_eq!(cfg.grid.aabb_size, Some([256.0, 256.0, 256.0]));
        assert!(cfg.grid.voxel_size.is_none());
    }

    #[test]
    fn test_load_config_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("bad.json");
        std::fs::write(&config_path, "not json").unwrap();

        assert!(load_config(&config_path).is_err());
    }

    #[test]
    fn test_resolve_no_config() {
        let cli = default_cli("out");
        let resolved = resolve_config(&cli, None).unwrap();
        assert!(resolved.shader.is_none());
        assert_eq!(resolved.out, PathBuf::from("out"));
        assert_eq!(resolved.bake_config.aabb_min, [0.0, 0.0, 0.0]);
        assert_eq!(resolved.bake_config.brick_size, 64);
    }

    #[test]
    fn test_resolve_config_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(
            &config_path,
            r#"{
                "shader": "test.wgsl",
                "grid": {
                    "aabb_min": [-32, -32, -32],
                    "aabb_size": [128, 128, 128],
                    "brick_size": 32
                },
                "mesh": { "adaptivity": 0.5 }
            }"#,
        )
        .unwrap();

        let cli = default_cli("out");
        let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

        // shader resolved relative to config dir
        assert_eq!(resolved.shader, Some(dir.path().join("test.wgsl")));
        // config values used (since CLI has defaults)
        assert_eq!(resolved.bake_config.aabb_min, [-32.0, -32.0, -32.0]);
        assert_eq!(resolved.bake_config.aabb_size, [128.0, 128.0, 128.0]);
        assert_eq!(resolved.bake_config.brick_size, 32);
        assert_eq!(resolved.bake_config.adaptivity, 0.5);
        // defaults for unset fields
        assert_eq!(resolved.bake_config.voxel_size, 1.0);
        assert_eq!(resolved.bake_config.iso, 0.0);
    }

    #[test]
    fn test_resolve_cli_shader_overrides_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(&config_path, r#"{ "shader": "config_shader.wgsl" }"#).unwrap();

        let mut cli = default_cli("out");
        cli.shader = Some(PathBuf::from("cli_shader.wgsl"));
        let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

        // CLI shader wins
        assert_eq!(resolved.shader, Some(PathBuf::from("cli_shader.wgsl")));
    }

    #[test]
    fn test_resolve_invalid_brick_size() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(&config_path, r#"{ "grid": { "brick_size": 16 } }"#).unwrap();

        let cli = default_cli("out");
        assert!(resolve_config(&cli, Some(&config_path)).is_err());
    }

    #[test]
    fn test_resolve_invalid_adaptivity() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(&config_path, r#"{ "mesh": { "adaptivity": 1.5 } }"#).unwrap();

        let cli = default_cli("out");
        assert!(resolve_config(&cli, Some(&config_path)).is_err());
    }

    #[test]
    fn test_resolve_genmesh_from_config() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(
            &config_path,
            r#"{ "genmesh": { "path": "tools/genmesh.exe", "write_vdb": true, "skip": true } }"#,
        )
        .unwrap();

        let cli = default_cli("out");
        let resolved = resolve_config(&cli, Some(&config_path)).unwrap();
        // genmesh.path is config-relative
        assert_eq!(
            resolved.genmesh_path,
            Some(dir.path().join("tools/genmesh.exe"))
        );
        assert!(resolved.write_vdb);
        assert!(resolved.skip_genmesh);
    }

    #[test]
    fn test_resolve_shader_relative_to_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).unwrap();
        let config_path = sub.join("cfg.json");
        std::fs::write(&config_path, r#"{ "shader": "sdf.wgsl" }"#).unwrap();

        let cli = default_cli("out");
        let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

        // shader path should be sub/sdf.wgsl
        assert_eq!(resolved.shader, Some(sub.join("sdf.wgsl")));
    }

    #[test]
    fn test_resolve_out_relative_to_config_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("project");
        std::fs::create_dir(&sub).unwrap();
        let config_path = sub.join("cfg.json");
        std::fs::write(&config_path, r#"{ "out": "output/test" }"#).unwrap();

        // CLI --out is not set
        let mut cli = default_cli("out");
        cli.out = None;
        let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

        // out should be resolved relative to config dir
        assert_eq!(resolved.out, sub.join("output/test"));
    }

    #[test]
    fn test_resolve_cli_out_is_cwd_relative() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("cfg.json");
        std::fs::write(&config_path, r#"{ "out": "config_output" }"#).unwrap();

        // CLI --out takes priority and is CWD-relative (not config-relative)
        let cli = default_cli("cli_output");
        let resolved = resolve_config(&cli, Some(&config_path)).unwrap();

        assert_eq!(resolved.out, PathBuf::from("cli_output"));
    }

    #[test]
    fn test_resolve_genmesh_path_env_fallback() {
        // With no explicit path and no env var, should fall back to PATH
        let result = resolve_genmesh_path(None);
        // In test env without PRINT_ORGANIZER_GENMESH set and no exe-adjacent genmesh,
        // should return bare "genmesh"
        assert!(
            result == PathBuf::from("genmesh") || result.exists(),
            "Expected 'genmesh' or an existing path, got {:?}",
            result
        );
    }

    #[test]
    fn test_resolve_genmesh_path_explicit() {
        let explicit = PathBuf::from("/custom/path/genmesh");
        let result = resolve_genmesh_path(Some(explicit.clone()));
        assert_eq!(result, explicit);
    }

    // ---- preset / v2 tests ----

    #[test]
    fn test_load_v1_no_presets() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("v1.json");
        std::fs::write(&p, r#"{ "shader": "sdf.wgsl" }"#).unwrap();
        let cfg = load_config(&p).unwrap();
        assert!(cfg.version.is_none());
        assert!(cfg.presets.is_none());
    }

    #[test]
    fn test_load_v2_with_presets() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("v2.json");
        std::fs::write(
            &p,
            r#"{
                "version": 2,
                "shader": "sdf.wgsl",
                "grid": { "aabb_size": [64, 64, 64], "voxel_size": 1.0 },
                "presets": [
                    { "name": "low", "grid": { "voxel_size": 2.0 } },
                    { "name": "high", "grid": { "voxel_size": 0.5 }, "mesh": { "adaptivity": 0.3 } }
                ]
            }"#,
        )
        .unwrap();
        let cfg = load_config(&p).unwrap();
        assert_eq!(cfg.version, Some(2));
        let presets = cfg.presets.as_ref().unwrap();
        assert_eq!(presets.len(), 2);
        assert_eq!(presets[0].name, "low");
        assert_eq!(presets[0].grid.as_ref().unwrap().voxel_size, Some(2.0));
        assert!(presets[0].mesh.is_none());
        assert_eq!(presets[1].name, "high");
        assert_eq!(
            presets[1].mesh.as_ref().unwrap().adaptivity,
            Some(0.3)
        );
    }

    #[test]
    fn test_merge_preset_overrides() {
        let base = ConfigFile {
            shader: Some("sdf.wgsl".into()),
            out: Some("output".into()),
            grid: GridConfig {
                aabb_min: Some([0.0, 0.0, 0.0]),
                aabb_size: Some([64.0, 64.0, 64.0]),
                voxel_size: Some(1.0),
                brick_size: Some(64),
            },
            mesh: MeshParams {
                iso: Some(0.0),
                adaptivity: Some(0.0),
            },
            ..Default::default()
        };

        let preset = PresetEntry {
            name: "high-res".into(),
            grid: Some(GridConfig {
                voxel_size: Some(0.2),
                ..Default::default()
            }),
            mesh: Some(MeshParams {
                adaptivity: Some(0.5),
                ..Default::default()
            }),
            ..Default::default()
        };

        let merged = merge_preset(&base, &preset);
        // Overridden fields
        assert_eq!(merged.grid.voxel_size, Some(0.2));
        assert_eq!(merged.mesh.adaptivity, Some(0.5));
        // Inherited fields
        assert_eq!(merged.shader.as_deref(), Some("sdf.wgsl"));
        assert_eq!(merged.out.as_deref(), Some("output"));
        assert_eq!(merged.grid.aabb_size, Some([64.0, 64.0, 64.0]));
        assert_eq!(merged.grid.aabb_min, Some([0.0, 0.0, 0.0]));
        assert_eq!(merged.grid.brick_size, Some(64));
        assert_eq!(merged.mesh.iso, Some(0.0));
    }

    #[test]
    fn test_merge_preset_inherits_all_when_empty() {
        let base = ConfigFile {
            shader: Some("base.wgsl".into()),
            grid: GridConfig {
                aabb_size: Some([128.0, 128.0, 128.0]),
                ..Default::default()
            },
            ..Default::default()
        };

        let preset = PresetEntry {
            name: "empty".into(),
            ..Default::default()
        };

        let merged = merge_preset(&base, &preset);
        assert_eq!(merged.shader.as_deref(), Some("base.wgsl"));
        assert_eq!(merged.grid.aabb_size, Some([128.0, 128.0, 128.0]));
    }

    #[test]
    fn test_merge_preset_shader_override() {
        let base = ConfigFile {
            shader: Some("base.wgsl".into()),
            ..Default::default()
        };

        let preset = PresetEntry {
            name: "alt".into(),
            shader: Some("alt.wgsl".into()),
            ..Default::default()
        };

        let merged = merge_preset(&base, &preset);
        assert_eq!(merged.shader.as_deref(), Some("alt.wgsl"));
    }

    #[test]
    fn test_merge_preset_out_override() {
        let base = ConfigFile {
            out: Some("base_out".into()),
            ..Default::default()
        };

        let preset = PresetEntry {
            name: "custom".into(),
            out: Some("preset_out".into()),
            ..Default::default()
        };

        let merged = merge_preset(&base, &preset);
        assert_eq!(merged.out.as_deref(), Some("preset_out"));
    }
}
