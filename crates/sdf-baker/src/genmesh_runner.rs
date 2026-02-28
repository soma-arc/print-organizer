use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Configuration for running genmesh.
pub struct GenmeshRunConfig {
    /// Path to the genmesh executable.
    pub genmesh_path: PathBuf,
    /// Output directory (contains manifest.json, bricks.*, and will receive mesh.stl).
    pub out_dir: PathBuf,
    /// Iso-surface value.
    pub iso: f32,
    /// Mesh adaptivity.
    pub adaptivity: f32,
    /// Whether to write VDB output.
    pub write_vdb: bool,
}

/// Result of a genmesh invocation.
#[derive(Debug)]
pub struct GenmeshResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub report: Option<Report>,
}

/// Parsed report.json from genmesh (report.v1.schema.json).
#[derive(Debug, Deserialize)]
pub struct Report {
    pub schema_version: i32,
    pub status: String,
    pub stage: String,
    #[serde(default)]
    pub started_at_utc: Option<String>,
    #[serde(default)]
    pub ended_at_utc: Option<String>,
    #[serde(default)]
    pub inputs: Option<ReportInputs>,
    pub timing_ms: TimingMs,
    pub stats: Stats,
    #[serde(default)]
    pub warnings: Vec<Diagnostic>,
    #[serde(default)]
    pub errors: Vec<ErrorDiagnostic>,
}

#[derive(Debug, Deserialize)]
pub struct ReportInputs {
    #[serde(default)]
    pub manifest_path: Option<String>,
    #[serde(default)]
    pub in_dir: Option<String>,
    #[serde(default)]
    pub bricks_path: Option<String>,
    #[serde(default)]
    pub dtype: Option<String>,
    #[serde(default)]
    pub brick_size: Option<u32>,
    #[serde(default)]
    pub dims: Option<[u32; 3]>,
    #[serde(default)]
    pub voxel_size: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct TimingMs {
    pub total: f64,
    #[serde(default)]
    pub validate: Option<f64>,
    #[serde(default)]
    pub read: Option<f64>,
    #[serde(default)]
    pub vdb_build: Option<f64>,
    #[serde(default)]
    pub meshing: Option<f64>,
    #[serde(default)]
    pub write: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Stats {
    #[serde(default)]
    pub aabb_min: Option<[f64; 3]>,
    #[serde(default)]
    pub aabb_max: Option<[f64; 3]>,
    #[serde(default)]
    pub brick_count: Option<u64>,
    pub triangle_count: u64,
    #[serde(default)]
    pub quad_count: Option<u64>,
    pub vertex_count: u64,
    #[serde(default)]
    pub degenerate_count: Option<u64>,
    #[serde(default)]
    pub mesh_aabb_min: Option<[f64; 3]>,
    #[serde(default)]
    pub mesh_aabb_max: Option<[f64; 3]>,
    #[serde(default)]
    pub active_voxel_count: Option<u64>,
    #[serde(default)]
    pub memory_usage_mb: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ErrorDiagnostic {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub hint: Option<String>,
    #[serde(default)]
    pub caused_by: Option<String>,
}

/// Run genmesh as a subprocess.
pub fn run_genmesh(config: &GenmeshRunConfig) -> Result<GenmeshResult> {
    let manifest_path = config.out_dir.join("manifest.json");

    if !config.genmesh_path.exists() {
        bail!(
            "genmesh executable not found: {}",
            config.genmesh_path.display()
        );
    }

    let mut cmd = Command::new(&config.genmesh_path);
    cmd.arg("--manifest")
        .arg(&manifest_path)
        .arg("--in")
        .arg(&config.out_dir)
        .arg("--out")
        .arg(&config.out_dir)
        .arg("--force")
        .arg("--write-stl")
        .arg("--iso")
        .arg(config.iso.to_string())
        .arg("--adaptivity")
        .arg(config.adaptivity.to_string());

    if config.write_vdb {
        cmd.arg("--write-vdb");
    }

    log::info!("Running genmesh: {:?}", cmd);

    let output = cmd
        .output()
        .context("Failed to execute genmesh subprocess")?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !stderr.is_empty() {
        for line in stderr.lines() {
            log::debug!("[genmesh] {line}");
        }
    }

    if exit_code != 0 {
        log::error!("genmesh exited with code {exit_code}");
        log::error!("stderr:\n{stderr}");
    }

    // Try to parse report.json
    let report = parse_report(&config.out_dir);

    if let Some(ref r) = report {
        log::info!(
            "genmesh report: status={}, triangles={}, vertices={}, total_ms={:.1}",
            r.status,
            r.stats.triangle_count,
            r.stats.vertex_count,
            r.timing_ms.total,
        );
    }

    if exit_code != 0 {
        bail!("genmesh failed with exit code {exit_code}");
    }

    Ok(GenmeshResult {
        exit_code,
        stdout,
        stderr,
        report,
    })
}

/// Parse report.json from the output directory, if it exists.
fn parse_report(out_dir: &Path) -> Option<Report> {
    let report_path = out_dir.join("report.json");
    if !report_path.exists() {
        log::warn!("report.json not found at {}", report_path.display());
        return None;
    }

    let content = std::fs::read_to_string(&report_path)
        .map_err(|e| log::warn!("Failed to read report.json: {e}"))
        .ok()?;

    serde_json::from_str::<Report>(&content)
        .map_err(|e| log::warn!("Failed to parse report.json: {e}"))
        .ok()
}
