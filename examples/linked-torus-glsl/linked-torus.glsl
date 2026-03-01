float sdf_torus(vec3 p, float R, float r) {
    vec2 q = vec2(length(p.xz) - R, p.y);
    return length(q) - r;
}

float sdf(vec3 p) {
    // Center x=49 places combined X span [11,117] centered in AABB [0,128]
    // (total span = 106mm: t1 [x-38,x+38], t2 offset +30 -> [x-8,x+68])
    vec3 c = vec3(49.0, 64.0, 64.0);

    // Horizontal torus
    float t1 = sdf_torus(p - c, 30.0, 8.0);

    // Vertical torus (rotated 90 deg, offset along X)
    vec3 q = p - c - vec3(30.0, 0.0, 0.0);
    vec3 rotated = vec3(q.x, q.z, q.y);
    float t2 = sdf_torus(rotated, 30.0, 8.0);

    return min(t1, t2);
}
