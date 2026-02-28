use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::json;

use crate::types::{BakeConfig, BrickResult};

/// Write manifest.json to `dir` based on `config`.
///
/// Returns the path to the written file.
pub fn write_manifest(dir: &Path, config: &BakeConfig) -> Result<PathBuf> {
    fs::create_dir_all(dir).context("Failed to create output directory")?;

    let manifest = json!({
        "version": 1,
        "coordinate_system": {
            "handedness": "right",
            "up_axis": "Y",
            "front_axis": "+Z"
        },
        "units": "mm",
        "aabb_min": config.aabb_min,
        "aabb_size": config.aabb_size,
        "voxel_size": config.voxel_size,
        "dims": config.dims,
        "sample_at": "voxel_center",
        "axis_order": "x-fastest",
        "distance_sign": "negative_inside_positive_outside",
        "iso": config.iso,
        "adaptivity": config.adaptivity,
        "narrow_band": {
            "half_width_voxels": config.half_width_voxels
        },
        "brick": {
            "size": config.brick_size
        },
        "dtype": config.dtype,
        "background_value_mm": config.background_value,
        "hashes": {},
        "generator": {
            "name": "sdf-baker",
            "version": env!("CARGO_PKG_VERSION")
        }
    });

    let path = dir.join("manifest.json");
    let json_str = serde_json::to_string_pretty(&manifest)
        .context("Failed to serialize manifest")?;
    fs::write(&path, &json_str).context("Failed to write manifest.json")?;

    log::info!("Wrote {}", path.display());
    Ok(path)
}

/// Write bricks.bin and bricks.index.json to `dir`.
///
/// Only non-background bricks are written to bricks.bin.
pub fn write_bricks(dir: &Path, config: &BakeConfig, bricks: &[BrickResult]) -> Result<()> {
    fs::create_dir_all(dir).context("Failed to create output directory")?;

    let bin_path = dir.join("bricks.bin");
    let index_path = dir.join("bricks.index.json");

    let b = config.brick_size;
    let voxels_per_brick = (b * b * b) as usize;
    let bytes_per_brick = voxels_per_brick * std::mem::size_of::<f32>();

    // Write bricks.bin — only active (non-background) bricks
    let mut bin_file = fs::File::create(&bin_path).context("Failed to create bricks.bin")?;
    let mut brick_entries = Vec::new();
    let mut offset: u64 = 0;

    for brick in bricks {
        if brick.is_background {
            continue;
        }

        assert_eq!(
            brick.values.len(),
            voxels_per_brick,
            "Brick ({},{},{}) has {} values, expected {}",
            brick.bx,
            brick.by,
            brick.bz,
            brick.values.len(),
            voxels_per_brick,
        );

        let bytes: &[u8] = bytemuck::cast_slice(&brick.values);
        bin_file
            .write_all(bytes)
            .context("Failed to write brick data")?;

        brick_entries.push(json!({
            "bx": brick.bx,
            "by": brick.by,
            "bz": brick.bz,
            "offset_bytes": offset,
            "payload_bytes": bytes_per_brick,
            "encoding": "raw"
        }));

        offset += bytes_per_brick as u64;
    }

    bin_file.flush().context("Failed to flush bricks.bin")?;

    // Write bricks.index.json
    let index = json!({
        "version": 1,
        "brick_size": config.brick_size,
        "dtype": config.dtype,
        "axis_order": "x-fastest",
        "dims": config.dims,
        "bricks": brick_entries
    });

    let json_str =
        serde_json::to_string_pretty(&index).context("Failed to serialize bricks index")?;
    fs::write(&index_path, &json_str).context("Failed to write bricks.index.json")?;

    log::info!(
        "Wrote {} ({} active bricks, {} bytes)",
        bin_path.display(),
        brick_entries.len(),
        offset,
    );
    log::info!("Wrote {}", index_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> BakeConfig {
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

    fn dummy_brick(bx: u32, by: u32, bz: u32, is_background: bool) -> BrickResult {
        let b = 64u32;
        let n = (b * b * b) as usize;
        BrickResult {
            bx,
            by,
            bz,
            values: vec![if is_background { 999.0 } else { 1.0 }; n],
            is_background,
        }
    }

    #[test]
    fn test_write_manifest_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config();
        let path = write_manifest(dir.path(), &config).unwrap();
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "manifest.json");
    }

    #[test]
    fn test_write_manifest_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config();
        write_manifest(dir.path(), &config).unwrap();

        let content = fs::read_to_string(dir.path().join("manifest.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(v["version"], 1);
        assert_eq!(v["units"], "mm");
        assert_eq!(v["voxel_size"], 1.0);
        assert_eq!(v["dims"], json!([64, 64, 64]));
        assert_eq!(v["aabb_min"], json!([0.0, 0.0, 0.0]));
        assert_eq!(v["aabb_size"], json!([64.0, 64.0, 64.0]));
        assert_eq!(v["sample_at"], "voxel_center");
        assert_eq!(v["axis_order"], "x-fastest");
        assert_eq!(v["distance_sign"], "negative_inside_positive_outside");
        assert_eq!(v["iso"], 0.0);
        assert_eq!(v["adaptivity"], 0.0);
        assert_eq!(v["narrow_band"]["half_width_voxels"], 3);
        assert_eq!(v["brick"]["size"], 64);
        assert_eq!(v["dtype"], "f32");
        assert_eq!(v["background_value_mm"], 3.0);
        assert!(v["hashes"].is_object());
        assert_eq!(v["coordinate_system"]["handedness"], "right");
        assert_eq!(v["coordinate_system"]["up_axis"], "Y");
        assert_eq!(v["coordinate_system"]["front_axis"], "+Z");
    }

    #[test]
    fn test_write_bricks_creates_files() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config();
        let bricks = vec![dummy_brick(0, 0, 0, false)];
        write_bricks(dir.path(), &config, &bricks).unwrap();

        assert!(dir.path().join("bricks.bin").exists());
        assert!(dir.path().join("bricks.index.json").exists());
    }

    #[test]
    fn test_write_bricks_bin_size() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config();
        let bricks = vec![
            dummy_brick(0, 0, 0, false),
            dummy_brick(1, 0, 0, true), // background — skipped
        ];
        write_bricks(dir.path(), &config, &bricks).unwrap();

        let bin_size = fs::metadata(dir.path().join("bricks.bin")).unwrap().len();
        let expected = 64u64 * 64 * 64 * 4; // 1 active brick
        assert_eq!(bin_size, expected);
    }

    #[test]
    fn test_write_bricks_index_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config();
        let bricks = vec![
            dummy_brick(0, 0, 0, false),
            dummy_brick(1, 0, 0, false),
        ];
        write_bricks(dir.path(), &config, &bricks).unwrap();

        let content = fs::read_to_string(dir.path().join("bricks.index.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(v["version"], 1);
        assert_eq!(v["brick_size"], 64);
        assert_eq!(v["dtype"], "f32");
        assert_eq!(v["axis_order"], "x-fastest");
        assert_eq!(v["dims"], json!([64, 64, 64]));

        let brick_arr = v["bricks"].as_array().unwrap();
        assert_eq!(brick_arr.len(), 2);

        // First brick
        assert_eq!(brick_arr[0]["bx"], 0);
        assert_eq!(brick_arr[0]["offset_bytes"], 0);
        assert_eq!(brick_arr[0]["payload_bytes"], 64 * 64 * 64 * 4);
        assert_eq!(brick_arr[0]["encoding"], "raw");

        // Second brick — offset should be after first
        let expected_offset = 64i64 * 64 * 64 * 4;
        assert_eq!(brick_arr[1]["bx"], 1);
        assert_eq!(brick_arr[1]["offset_bytes"], expected_offset);
    }

    #[test]
    fn test_write_bricks_background_only() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config();
        let bricks = vec![dummy_brick(0, 0, 0, true)];
        write_bricks(dir.path(), &config, &bricks).unwrap();

        let bin_size = fs::metadata(dir.path().join("bricks.bin")).unwrap().len();
        assert_eq!(bin_size, 0, "Background-only should produce empty bricks.bin");

        let content = fs::read_to_string(dir.path().join("bricks.index.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        let brick_arr = v["bricks"].as_array().unwrap();
        assert_eq!(brick_arr.len(), 0);
    }
}
