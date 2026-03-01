fn sdf_torus(p: vec3<f32>, R: f32, r: f32) -> f32 {
    let q = vec2<f32>(length(p.xz) - R, p.y);
    return length(q) - r;
}

fn sdf(p: vec3<f32>) -> f32 {
    // Center x=49 places combined X span [11,117] centered in AABB [0,128]
    // (total span = 106mm: t1 [x-38,x+38], t2 offset +30 -> [x-8,x+68])
    let c = vec3<f32>(49.0, 64.0, 64.0);

    // Horizontal torus
    let t1 = sdf_torus(p - c, 30.0, 8.0);

    // Vertical torus (rotated 90 deg, offset along X)
    let q = p - c - vec3<f32>(30.0, 0.0, 0.0);
    let rotated = vec3<f32>(q.x, q.z, q.y);
    let t2 = sdf_torus(rotated, 30.0, 8.0);

    return min(t1, t2);
}
