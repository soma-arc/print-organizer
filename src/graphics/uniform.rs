#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GlobalsUniform {
    pub resolution: [f32; 2],
    pub time: f32,
    pub _pad: [f32; 1],
}

impl GlobalsUniform {
    pub fn new(width: u32, height: u32, time: f32) -> Self {
        Self {
            resolution: [width as f32, height as f32],
            time,
            _pad: [0.0],
        }
    }
}
