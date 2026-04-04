use std::path::PathBuf;

use clap::Parser;

/// SDF shader → GPU compute → genmesh bricks → STL mesh pipeline.
#[derive(Parser, Debug)]
#[command(name = "sdf-baker", version, about)]
pub struct Cli {
    /// Path to JSON config file. All other args can be specified in the file.
    /// CLI arguments override config file values.
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Path to user SDF shader file (WGSL or GLSL).
    /// If omitted, the built-in sphere SDF is used.
    #[arg(long)]
    pub shader: Option<PathBuf>,

    /// Output directory for bricks and mesh files.
    #[arg(long, required_unless_present = "config")]
    pub out: Option<PathBuf>,

    /// AABB minimum corner (x,y,z).
    #[arg(long, default_value = "0,0,0", value_parser = parse_vec3)]
    pub aabb_min: [f32; 3],

    /// AABB size in each axis (x,y,z).
    #[arg(long, default_value = "64,64,64", value_parser = parse_vec3)]
    pub aabb_size: [f32; 3],

    /// Voxel edge length in world units.
    #[arg(long, default_value_t = 1.0)]
    pub voxel_size: f32,

    /// Brick edge length in voxels. Must be 32, 64, or 128.
    #[arg(long, default_value_t = 64, value_parser = parse_brick_size)]
    pub brick_size: u32,

    /// Narrow-band half-width in voxels.
    #[arg(long, default_value_t = 3)]
    pub half_width: u32,

    /// Iso-surface value.
    #[arg(long, default_value_t = 0.0)]
    pub iso: f32,

    /// Mesh simplification adaptivity (0.0 = no simplification).
    #[arg(long, default_value_t = 0.0)]
    pub adaptivity: f32,

    /// Level set offset for dilation (positive) or erosion (negative) in mm.
    #[arg(long, default_value_t = 0.0)]
    pub offset_mm: f32,

    /// Data type for distance values.
    #[arg(long, default_value = "f32", value_parser = ["f32", "f16"])]
    pub dtype: String,

    /// Path to the genmesh executable.
    #[arg(long)]
    pub genmesh_path: Option<PathBuf>,

    /// Only write bricks output; do not invoke genmesh.
    #[arg(long, default_value_t = false)]
    pub skip_genmesh: bool,

    /// Pass --write-vdb to genmesh.
    #[arg(long, default_value_t = false)]
    pub write_vdb: bool,

    /// Overwrite output directory if it already exists.
    #[arg(long, default_value_t = false)]
    pub force: bool,

    /// Log verbosity level.
    #[arg(long, default_value = "info", value_parser = ["error", "warn", "info", "debug"])]
    pub log_level: String,
}

fn parse_vec3(s: &str) -> Result<[f32; 3], String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 3 {
        return Err(format!(
            "expected 3 comma-separated floats, got {}",
            parts.len()
        ));
    }
    let x = parts[0]
        .trim()
        .parse::<f32>()
        .map_err(|e| format!("x: {e}"))?;
    let y = parts[1]
        .trim()
        .parse::<f32>()
        .map_err(|e| format!("y: {e}"))?;
    let z = parts[2]
        .trim()
        .parse::<f32>()
        .map_err(|e| format!("z: {e}"))?;
    Ok([x, y, z])
}

fn parse_brick_size(s: &str) -> Result<u32, String> {
    let v: u32 = s.parse().map_err(|e| format!("{e}"))?;
    if v == 32 || v == 64 || v == 128 {
        Ok(v)
    } else {
        Err(format!("brick-size must be 32, 64, or 128, got {v}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_vec3_valid() {
        assert_eq!(parse_vec3("1.0,2.5,3.0").unwrap(), [1.0, 2.5, 3.0]);
    }

    #[test]
    fn test_parse_vec3_with_spaces() {
        assert_eq!(parse_vec3(" 1 , 2 , 3 ").unwrap(), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_parse_vec3_wrong_count() {
        assert!(parse_vec3("1,2").is_err());
    }

    #[test]
    fn test_parse_brick_size_valid() {
        assert_eq!(parse_brick_size("32").unwrap(), 32);
        assert_eq!(parse_brick_size("64").unwrap(), 64);
        assert_eq!(parse_brick_size("128").unwrap(), 128);
    }

    #[test]
    fn test_parse_brick_size_invalid() {
        assert!(parse_brick_size("16").is_err());
        assert!(parse_brick_size("100").is_err());
    }

    #[test]
    fn test_cli_defaults() {
        let cli = Cli::parse_from(["sdf-baker", "--out", "output"]);
        assert!(cli.config.is_none());
        assert_eq!(cli.out, Some(PathBuf::from("output")));
        assert_eq!(cli.aabb_min, [0.0, 0.0, 0.0]);
        assert_eq!(cli.aabb_size, [64.0, 64.0, 64.0]);
        assert_eq!(cli.voxel_size, 1.0);
        assert_eq!(cli.brick_size, 64);
        assert_eq!(cli.half_width, 3);
        assert_eq!(cli.iso, 0.0);
        assert_eq!(cli.adaptivity, 0.0);
        assert_eq!(cli.dtype, "f32");
        assert!(cli.shader.is_none());
        assert!(cli.genmesh_path.is_none());
        assert!(!cli.skip_genmesh);
        assert!(!cli.write_vdb);
        assert!(!cli.force);
        assert_eq!(cli.log_level, "info");
    }

    #[test]
    fn test_cli_config_without_out() {
        // --config can be used without --out
        let cli = Cli::parse_from(["sdf-baker", "--config", "my.json"]);
        assert_eq!(cli.config, Some(PathBuf::from("my.json")));
        assert!(cli.out.is_none());
    }

    #[test]
    fn test_cli_all_args() {
        let cli = Cli::parse_from([
            "sdf-baker",
            "--config",
            "cfg.json",
            "--shader",
            "my.wgsl",
            "--out",
            "out",
            "--aabb-min",
            "1,2,3",
            "--aabb-size",
            "10,20,30",
            "--voxel-size",
            "0.5",
            "--brick-size",
            "32",
            "--half-width",
            "5",
            "--iso",
            "0.1",
            "--adaptivity",
            "0.5",
            "--dtype",
            "f16",
            "--genmesh-path",
            "genmesh.exe",
            "--skip-genmesh",
            "--write-vdb",
            "--force",
            "--log-level",
            "debug",
        ]);
        assert_eq!(cli.shader.unwrap().to_str().unwrap(), "my.wgsl");
        assert_eq!(cli.aabb_min, [1.0, 2.0, 3.0]);
        assert_eq!(cli.aabb_size, [10.0, 20.0, 30.0]);
        assert_eq!(cli.voxel_size, 0.5);
        assert_eq!(cli.brick_size, 32);
        assert_eq!(cli.half_width, 5);
        assert_eq!(cli.iso, 0.1);
        assert_eq!(cli.adaptivity, 0.5);
        assert_eq!(cli.dtype, "f16");
        assert!(cli.skip_genmesh);
        assert!(cli.write_vdb);
        assert!(cli.force);
        assert_eq!(cli.log_level, "debug");
    }
}
