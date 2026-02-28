// --- raymarch_template.wgsl ---
// System-generated render shader for SDF preview (ray marching).
// The user's sdf() function is inserted below by shader composition.

struct Globals {
    camera_pos:    vec3<f32>,
    _pad0:         f32,
    camera_target: vec3<f32>,
    _pad1:         f32,
    camera_up:     vec3<f32>,
    _pad2:         f32,
    aabb_min:      vec3<f32>,
    _pad3:         f32,
    aabb_size:     vec3<f32>,
    _pad4:         f32,
    resolution:    vec2<f32>,
    time:          f32,
    _pad5:         f32,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

// ----------- vertex shader (fullscreen quad) -----------

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );
    let p = positions[vi];
    var out: VertexOutput;
    out.clip_position = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

// ----------- user SDF (injected) -----------

{{USER_SDF}}

// ----------- ray marching -----------

const MAX_STEPS: i32 = 256;
const MIN_DIST: f32 = 0.001;

fn calc_normal(p: vec3<f32>, eps: f32) -> vec3<f32> {
    let e = vec2<f32>(eps, 0.0);
    let n = vec3<f32>(
        sdf(p + e.xyy) - sdf(p - e.xyy),
        sdf(p + e.yxy) - sdf(p - e.yxy),
        sdf(p + e.yyx) - sdf(p - e.yyx),
    );
    return normalize(n);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // --- camera setup ---
    let forward = normalize(globals.camera_target - globals.camera_pos);
    let right   = normalize(cross(forward, globals.camera_up));
    let up      = cross(right, forward);

    let aspect = globals.resolution.x / globals.resolution.y;
    let ndc = vec2<f32>(
        (in.uv.x - 0.5) * 2.0 * aspect,
        (in.uv.y - 0.5) * 2.0,
    );

    let fov_factor = 1.0;  // tan(45°) = 1.0
    let ray_dir = normalize(forward * fov_factor + right * ndc.x + up * ndc.y);

    // --- AABB bounding ---
    let aabb_max = globals.aabb_min + globals.aabb_size;
    let diag = length(globals.aabb_size);
    let max_dist = diag * 3.0;

    // --- ray march ---
    var t = 0.0;
    var hit = false;
    for (var i = 0; i < MAX_STEPS; i = i + 1) {
        let p = globals.camera_pos + ray_dir * t;
        let d = sdf(p);
        if (d < MIN_DIST) {
            hit = true;
            break;
        }
        t = t + d;
        if (t > max_dist) {
            break;
        }
    }

    if (!hit) {
        // background gradient: dark gray top, slightly lighter bottom
        let grad = mix(0.18, 0.12, in.uv.y);
        return vec4<f32>(grad, grad, grad, 1.0);
    }

    let hit_pos = globals.camera_pos + ray_dir * t;

    // --- shading (Phong) ---
    let normal = calc_normal(hit_pos, MIN_DIST * 2.0);

    // Key light: from upper-right-front
    let light_dir = normalize(vec3<f32>(0.5, 0.8, 0.6));
    let diffuse = max(dot(normal, light_dir), 0.0);

    // Ambient
    let ambient = 0.15;

    // Specular (Blinn-Phong)
    let view_dir = normalize(globals.camera_pos - hit_pos);
    let half_vec = normalize(light_dir + view_dir);
    let spec = pow(max(dot(normal, half_vec), 0.0), 32.0) * 0.4;

    // Hemisphere ambient: subtle sky/ground
    let sky_ambient = mix(0.08, 0.02, normal.y * 0.5 + 0.5);

    let base_color = vec3<f32>(0.8, 0.75, 0.7);
    let color = base_color * (ambient + diffuse + sky_ambient) + vec3<f32>(spec);

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
