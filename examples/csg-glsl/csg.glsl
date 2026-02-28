float sdf(vec3 p) {
    vec3 center = vec3(64.0, 64.0, 64.0);
    vec3 q = p - center;

    // Sphere: radius 40
    float sphere = length(q) - 40.0;

    // Rounded box: half-extent 25
    vec3 half_ext = vec3(25.0, 25.0, 25.0);
    vec3 d = abs(q) - half_ext;
    float box_d = length(max(d, vec3(0.0))) + min(max(d.x, max(d.y, d.z)), 0.0);

    // Subtract box from sphere
    return max(sphere, -box_d);
}
