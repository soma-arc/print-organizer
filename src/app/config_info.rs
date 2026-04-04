#[derive(Debug, Clone)]
pub(super) struct ConfigInfo {
    pub shader: String,
    pub aabb_min: [f32; 3],
    pub aabb_size: [f32; 3],
    pub voxel_size: f32,
    pub brick_size: u32,
    pub dims: [u32; 3],
    pub brick_counts: [u32; 3],
    pub total_voxels: u64,
    pub half_width: u32,
    pub iso: f32,
    pub adaptivity: f32,
    pub offset_mm: f32,
    pub genmesh_path: String,
    pub write_vdb: bool,
}

impl ConfigInfo {
    pub fn from_config(cfg: &sdf_baker::config::ConfigFile, cfg_dir: &std::path::Path) -> Self {
        let aabb_min = cfg.grid.aabb_min.unwrap_or([0.0, 0.0, 0.0]);
        let aabb_size = cfg.grid.aabb_size.unwrap_or([64.0, 64.0, 64.0]);
        let voxel_size = cfg.grid.voxel_size.unwrap_or(1.0);
        let brick_size = cfg.grid.brick_size.unwrap_or(64);
        let half_width = cfg.bake.half_width.unwrap_or(3);

        let dims = [
            (aabb_size[0] / voxel_size).ceil() as u32,
            (aabb_size[1] / voxel_size).ceil() as u32,
            (aabb_size[2] / voxel_size).ceil() as u32,
        ];
        let brick_counts = [
            (dims[0] + brick_size - 1) / brick_size,
            (dims[1] + brick_size - 1) / brick_size,
            (dims[2] + brick_size - 1) / brick_size,
        ];
        let total_voxels = dims[0] as u64 * dims[1] as u64 * dims[2] as u64;

        let shader = cfg
            .shader
            .as_ref()
            .map(|s| cfg_dir.join(s).display().to_string())
            .unwrap_or_else(|| "(built-in sphere)".into());

        ConfigInfo {
            shader,
            aabb_min,
            aabb_size,
            voxel_size,
            brick_size,
            dims,
            brick_counts,
            total_voxels,
            half_width,
            iso: cfg.mesh.iso.unwrap_or(0.0),
            adaptivity: cfg.mesh.adaptivity.unwrap_or(0.0),
            offset_mm: cfg.mesh.offset_mm.unwrap_or(0.0),
            genmesh_path: {
                let explicit = cfg.genmesh.path.as_ref().map(|p| cfg_dir.join(p));
                sdf_baker::config::resolve_genmesh_path(explicit)
                    .display()
                    .to_string()
            },
            write_vdb: cfg.genmesh.write_vdb.unwrap_or(false),
        }
    }
}
