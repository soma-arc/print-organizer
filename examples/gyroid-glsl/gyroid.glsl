float sdf(vec3 p) {
    float s = 0.1;
    float thickness = 0.3;
    float g = sin(p.x * s) * cos(p.y * s)
            + sin(p.y * s) * cos(p.z * s)
            + sin(p.z * s) * cos(p.x * s);
    return abs(g) - thickness;
}
