pub(super) struct OrbitCamera {
    pub target: [f32; 3],
    pub yaw: f32,   // radians
    pub pitch: f32, // radians
    pub distance: f32,
}

impl OrbitCamera {
    pub fn from_aabb(aabb_min: [f32; 3], aabb_size: [f32; 3]) -> Self {
        let cx = aabb_min[0] + aabb_size[0] * 0.5;
        let cy = aabb_min[1] + aabb_size[1] * 0.5;
        let cz = aabb_min[2] + aabb_size[2] * 0.5;
        let diag = (aabb_size[0] * aabb_size[0]
            + aabb_size[1] * aabb_size[1]
            + aabb_size[2] * aabb_size[2])
            .sqrt();
        Self {
            target: [cx, cy, cz],
            yaw: std::f32::consts::FRAC_PI_4, // 45°
            pitch: 0.5236,                    // 30°
            distance: diag * 1.5,
        }
    }

    pub fn position(&self) -> [f32; 3] {
        let cos_p = self.pitch.cos();
        let sin_p = self.pitch.sin();
        let cos_y = self.yaw.cos();
        let sin_y = self.yaw.sin();
        [
            self.target[0] + self.distance * cos_p * sin_y,
            self.target[1] + self.distance * sin_p,
            self.target[2] + self.distance * cos_p * cos_y,
        ]
    }
}
