float sdf_torus(vec3 p, float R, float r) {
    vec2 q = vec2(length(p.xz) - R, p.y);
    return length(q) - r;
}

float sdf(vec3 p) {
    vec3 c = vec3(64.0, 64.0, 64.0);

    // Horizontal torus
    float t1 = sdf_torus(p - c, 30.0, 8.0);

    // Vertical torus (rotated 90 deg, offset along X)
    vec3 q = p - c - vec3(30.0, 0.0, 0.0);
    vec3 rotated = vec3(q.x, q.z, q.y);
    float t2 = sdf_torus(rotated, 30.0, 8.0);

    return min(t1, t2);
}
