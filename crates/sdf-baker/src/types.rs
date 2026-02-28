use bytemuck::{Pod, Zeroable};
use serde::Serialize;

/// GPU uniform parameters passed to the compute shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ComputeParams {
    /// AABB minimum corner in world coordinates.
    pub aabb_min: [f32; 3],
    /// Voxel edge length in world units.
    pub voxel_size: f32,
    /// Brick offset in voxel coordinates (bx*B, by*B, bz*B).
    pub brick_offset: [u32; 3],
    /// Brick edge length in voxels.
    pub brick_size: u32,
}

/// Configuration for a bake operation.
#[derive(Debug, Clone, Serialize)]
pub struct BakeConfig {
    pub aabb_min: [f32; 3],
    pub aabb_size: [f32; 3],
    pub voxel_size: f32,
    /// Grid dimensions in voxels (computed from aabb_size / voxel_size, rounded up).
    pub dims: [u32; 3],
    /// Brick edge length in voxels.
    pub brick_size: u32,
    pub half_width_voxels: u32,
    pub iso: f32,
    pub adaptivity: f32,
    pub dtype: String,
    /// Background (far-field) distance value in world units.
    pub background_value: f32,
}

impl BakeConfig {
    /// Create a BakeConfig from CLI-level parameters.
    pub fn new(
        aabb_min: [f32; 3],
        aabb_size: [f32; 3],
        voxel_size: f32,
        brick_size: u32,
        half_width_voxels: u32,
        iso: f32,
        adaptivity: f32,
        dtype: String,
    ) -> Self {
        let dims = [
            (aabb_size[0] / voxel_size).ceil() as u32,
            (aabb_size[1] / voxel_size).ceil() as u32,
            (aabb_size[2] / voxel_size).ceil() as u32,
        ];
        let background_value = half_width_voxels as f32 * voxel_size;
        Self {
            aabb_min,
            aabb_size,
            voxel_size,
            dims,
            brick_size,
            half_width_voxels,
            iso,
            adaptivity,
            dtype,
            background_value,
        }
    }

    /// Number of bricks along each axis.
    pub fn brick_counts(&self) -> [u32; 3] {
        [
            (self.dims[0] + self.brick_size - 1) / self.brick_size,
            (self.dims[1] + self.brick_size - 1) / self.brick_size,
            (self.dims[2] + self.brick_size - 1) / self.brick_size,
        ]
    }
}

/// Result of baking a single brick.
#[derive(Debug)]
pub struct BrickResult {
    /// Brick coordinate indices.
    pub bx: u32,
    pub by: u32,
    pub bz: u32,
    /// Evaluated SDF values (length = brick_size^3), x-fastest order.
    pub values: Vec<f32>,
    /// True if all values are at background distance (brick can be skipped).
    pub is_background: bool,
}

/// Complete output from a bake operation.
#[derive(Debug)]
pub struct BakeOutput {
    pub config: BakeConfig,
    pub bricks: Vec<BrickResult>,
    /// Wall-clock time for the GPU bake in seconds.
    pub bake_time_secs: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bake_config_dims() {
        let cfg = BakeConfig::new(
            [0.0, 0.0, 0.0],
            [64.0, 64.0, 64.0],
            1.0,
            64,
            3,
            0.0,
            0.0,
            "f32".to_string(),
        );
        assert_eq!(cfg.dims, [64, 64, 64]);
        assert_eq!(cfg.brick_counts(), [1, 1, 1]);
    }

    #[test]
    fn test_bake_config_dims_non_aligned() {
        let cfg = BakeConfig::new(
            [0.0, 0.0, 0.0],
            [100.0, 50.0, 70.0],
            1.0,
            64,
            3,
            0.0,
            0.0,
            "f32".to_string(),
        );
        assert_eq!(cfg.dims, [100, 50, 70]);
        assert_eq!(cfg.brick_counts(), [2, 1, 2]); // ceil(100/64)=2, ceil(50/64)=1, ceil(70/64)=2
    }

    #[test]
    fn test_bake_config_background_value() {
        let cfg = BakeConfig::new(
            [0.0, 0.0, 0.0],
            [64.0, 64.0, 64.0],
            0.5,
            64,
            3,
            0.0,
            0.0,
            "f32".to_string(),
        );
        assert_eq!(cfg.background_value, 1.5); // 3 * 0.5
    }

    #[test]
    fn test_compute_params_is_pod() {
        // Verify ComputeParams is valid for GPU buffer usage
        let params = ComputeParams {
            aabb_min: [0.0, 0.0, 0.0],
            voxel_size: 1.0,
            brick_offset: [0, 0, 0],
            brick_size: 64,
        };
        let bytes = bytemuck::bytes_of(&params);
        assert_eq!(bytes.len(), 32); // 3*4 + 4 + 3*4 + 4 = 32 bytes
    }
}
