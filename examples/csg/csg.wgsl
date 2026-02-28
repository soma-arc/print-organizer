fn sdf(p: vec3<f32>) -> f32 {
    let center = vec3<f32>(64.0, 64.0, 64.0);
    let q = p - center;

    // Sphere: radius 40
    let sphere = length(q) - 40.0;

    // Rounded box: half-extent 25
    let half = vec3<f32>(25.0, 25.0, 25.0);
    let d = abs(q) - half;
    let box_d = length(max(d, vec3<f32>(0.0))) + min(max(d.x, max(d.y, d.z)), 0.0);

    // Subtract box from sphere
    return max(sphere, -box_d);
}
