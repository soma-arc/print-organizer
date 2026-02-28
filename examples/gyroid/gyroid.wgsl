fn sdf(p: vec3<f32>) -> f32 {
    let s = 0.1;
    let thickness = 0.5;
    let g = sin(p.x * s) * cos(p.y * s)
          + sin(p.y * s) * cos(p.z * s)
          + sin(p.z * s) * cos(p.x * s);
    return abs(g) - thickness;
}
