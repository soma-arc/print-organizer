/// Uniform buffer matching the `Globals` struct in raymarch_template.wgsl.
///
/// Layout (std140-aligned):
/// ```text
///  0: camera_pos    (vec3 + pad)   16 bytes
/// 16: camera_target (vec3 + pad)   16 bytes
/// 32: camera_up     (vec3 + pad)   16 bytes
/// 48: aabb_min      (vec3 + pad)   16 bytes
/// 64: aabb_size     (vec3 + pad)   16 bytes
/// 80: resolution    (vec2)          8 bytes
/// 88: time          (f32)           4 bytes
/// 92: _pad5         (f32)           4 bytes
/// Total: 96 bytes
/// ```
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlobalsUniform {
    pub camera_pos: [f32; 3],
    pub _pad0: f32,
    pub camera_target: [f32; 3],
    pub _pad1: f32,
    pub camera_up: [f32; 3],
    pub _pad2: f32,
    pub aabb_min: [f32; 3],
    pub _pad3: f32,
    pub aabb_size: [f32; 3],
    pub _pad4: f32,
    pub resolution: [f32; 2],
    pub time: f32,
    pub _pad5: f32,
}

impl GlobalsUniform {
    pub fn new(
        camera_pos: [f32; 3],
        camera_target: [f32; 3],
        camera_up: [f32; 3],
        aabb_min: [f32; 3],
        aabb_size: [f32; 3],
        width: u32,
        height: u32,
        time: f32,
    ) -> Self {
        Self {
            camera_pos,
            _pad0: 0.0,
            camera_target,
            _pad1: 0.0,
            camera_up,
            _pad2: 0.0,
            aabb_min,
            _pad3: 0.0,
            aabb_size,
            _pad4: 0.0,
            resolution: [width as f32, height as f32],
            time,
            _pad5: 0.0,
        }
    }
}
