float sdf(vec3 p) {
    return length(p - vec3(32.0, 32.0, 32.0)) - 25.6;
}
