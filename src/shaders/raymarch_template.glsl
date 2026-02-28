// --- raymarch_template.glsl ---
// System-generated render shader for SDF preview (ray marching).
// The user's sdf() function is inserted below by shader composition.
// This is a fragment shader; naga converts it to WGSL after composition.

#version 450

layout(std140, set = 0, binding = 0) uniform Globals {
    vec3 camera_pos;
    float _pad0;
    vec3 camera_target;
    float _pad1;
    vec3 camera_up;
    float _pad2;
    vec3 aabb_min;
    float _pad3;
    vec3 aabb_size;
    float _pad4;
    vec2 resolution;
    float time;
    float brick_size;
    uint show_aabb;
    uint show_bricks;
    uvec2 _pad6;
} globals;

layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 frag_color;

{{USER_SDF}}

const int   MAX_STEPS = 256;
const float MIN_DIST  = 0.001;

vec3 calc_normal(vec3 p, float eps) {
    vec2 e = vec2(eps, 0.0);
    vec3 n = vec3(
        sdf(p + e.xyy) - sdf(p - e.xyy),
        sdf(p + e.yxy) - sdf(p - e.yxy),
        sdf(p + e.yyx) - sdf(p - e.yyx)
    );
    return normalize(n);
}

void main() {
    // --- camera setup ---
    vec3 forward = normalize(globals.camera_target - globals.camera_pos);
    vec3 right   = normalize(cross(forward, globals.camera_up));
    vec3 up      = cross(right, forward);

    float aspect = globals.resolution.x / globals.resolution.y;
    vec2 ndc = vec2(
        (uv.x - 0.5) * 2.0 * aspect,
        (uv.y - 0.5) * 2.0
    );

    float fov_factor = 1.0;  // tan(45°) = 1.0
    vec3 ray_dir = normalize(forward * fov_factor + right * ndc.x + up * ndc.y);

    // --- AABB bounding ---
    vec3 aabb_max = globals.aabb_min + globals.aabb_size;
    float diag = length(globals.aabb_size);
    float max_dist = diag * 3.0;

    // --- ray march ---
    float t = 0.0;
    bool hit = false;
    for (int i = 0; i < MAX_STEPS; i++) {
        vec3 p = globals.camera_pos + ray_dir * t;
        float d = sdf(p);
        if (d < MIN_DIST) {
            hit = true;
            break;
        }
        t += d;
        if (t > max_dist) {
            break;
        }
    }

    if (!hit) {
        // background gradient: dark gray top, slightly lighter bottom
        float grad = mix(0.18, 0.12, uv.y);
        frag_color = vec4(grad, grad, grad, 1.0);
        return;
    }

    vec3 hit_pos = globals.camera_pos + ray_dir * t;

    // --- shading (Phong) ---
    vec3 normal = calc_normal(hit_pos, MIN_DIST * 2.0);

    // Key light: from upper-right-front
    vec3 light_dir = normalize(vec3(0.5, 0.8, 0.6));
    float diffuse = max(dot(normal, light_dir), 0.0);

    // Ambient
    float ambient = 0.15;

    // Specular (Blinn-Phong)
    vec3 view_dir = normalize(globals.camera_pos - hit_pos);
    vec3 half_vec = normalize(light_dir + view_dir);
    float spec = pow(max(dot(normal, half_vec), 0.0), 32.0) * 0.4;

    // Hemisphere ambient: subtle sky/ground
    float sky_ambient = mix(0.08, 0.02, normal.y * 0.5 + 0.5);

    vec3 base_color = vec3(0.8, 0.75, 0.7);
    vec3 color = base_color * (ambient + diffuse + sky_ambient) + vec3(spec);

    frag_color = vec4(clamp(color, vec3(0.0), vec3(1.0)), 1.0);
}
