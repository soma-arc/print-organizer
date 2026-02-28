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
    brick_size:    f32,
    show_aabb:     u32,
    show_bricks:   u32,
    clip_aabb:     u32,
    _pad6:         u32,
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

/// Signed distance to the interior of an axis-aligned bounding box.
/// Negative inside, positive outside.
fn aabb_sdf(p: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>) -> f32 {
    let center = (box_min + box_max) * 0.5;
    let half = (box_max - box_min) * 0.5;
    let q = abs(p - center) - half;
    return length(max(q, vec3<f32>(0.0))) + min(max(q.x, max(q.y, q.z)), 0.0);
}

/// Scene SDF: user sdf, optionally clipped to AABB.
fn scene_sdf(p: vec3<f32>) -> f32 {
    let d = sdf(p);
    if (globals.clip_aabb != 0u) {
        let aabb_max = globals.aabb_min + globals.aabb_size;
        let d_box = aabb_sdf(p, globals.aabb_min, aabb_max);
        return max(d, d_box);
    }
    return d;
}

fn calc_normal(p: vec3<f32>, eps: f32) -> vec3<f32> {
    let e = vec2<f32>(eps, 0.0);
    let n = vec3<f32>(
        scene_sdf(p + e.xyy) - scene_sdf(p - e.xyy),
        scene_sdf(p + e.yxy) - scene_sdf(p - e.yxy),
        scene_sdf(p + e.yyx) - scene_sdf(p - e.yyx),
    );
    return normalize(n);
}

/// Distance to the edges of an axis-aligned box (wireframe).
/// Returns small value near any of the 12 edges.
fn box_frame_dist(p: vec3<f32>, box_min: vec3<f32>, box_max: vec3<f32>, thickness: f32) -> f32 {
    let q = p - box_min;
    let s = box_max - box_min;

    // For each axis, how far from the nearest face pair along the other two axes
    let dx = min(abs(q.x), abs(q.x - s.x));
    let dy = min(abs(q.y), abs(q.y - s.y));
    let dz = min(abs(q.z), abs(q.z - s.z));

    // An edge is where two of the three face-distances are small
    let edge_xy = max(dx, dy);
    let edge_yz = max(dy, dz);
    let edge_xz = max(dx, dz);

    return min(min(edge_xy, edge_yz), edge_xz) - thickness;
}

/// Distance to all brick boundary edges within the AABB.
fn bricks_frame_dist(p: vec3<f32>, aabb_min: vec3<f32>, aabb_size: vec3<f32>, voxel_brick: f32, thickness: f32) -> f32 {
    // Map point into AABB-relative coords, then into brick-cell-relative coords
    let rel = p - aabb_min;
    // Fractional position within the current brick cell
    let cell = rel / voxel_brick;
    let frac = fract(cell);
    let local = frac * voxel_brick;

    // Distance to each face pair of the brick cell
    let dx = min(local.x, voxel_brick - local.x);
    let dy = min(local.y, voxel_brick - local.y);
    let dz = min(local.z, voxel_brick - local.z);

    let edge_xy = max(dx, dy);
    let edge_yz = max(dy, dz);
    let edge_xz = max(dx, dz);

    return min(min(edge_xy, edge_yz), edge_xz) - thickness;
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
    let edge_thickness = diag * 0.002;  // scale-independent edge width
    let brick_world = globals.brick_size;  // brick size in world units

    // --- ray march ---
    var t = 0.0;
    var hit = false;
    for (var i = 0; i < MAX_STEPS; i = i + 1) {
        let p = globals.camera_pos + ray_dir * t;
        let d = scene_sdf(p);
        if (d < MIN_DIST) {
            hit = true;
            break;
        }
        t = t + d;
        if (t > max_dist) {
            break;
        }
    }

    // --- background ---
    var color: vec3<f32>;
    var alpha = 1.0;

    if (!hit) {
        // background gradient: dark gray top, slightly lighter bottom
        let grad = mix(0.18, 0.12, in.uv.y);
        color = vec3<f32>(grad, grad, grad);
    } else {
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
        color = base_color * (ambient + diffuse + sky_ambient) + vec3<f32>(spec);
    }

    // --- AABB / brick overlays (drawn via secondary ray march) ---
    if (globals.show_aabb != 0u || globals.show_bricks != 0u) {
        // Quick ray-AABB intersection to limit overlay march
        let inv_dir = 1.0 / ray_dir;
        let t1 = (globals.aabb_min - globals.camera_pos) * inv_dir;
        let t2 = (aabb_max - globals.camera_pos) * inv_dir;
        let tmin_v = min(t1, t2);
        let tmax_v = max(t1, t2);
        let t_enter = max(max(tmin_v.x, tmin_v.y), tmin_v.z);
        let t_exit  = min(min(tmax_v.x, tmax_v.y), tmax_v.z);

        if (t_exit > max(t_enter, 0.0)) {
            let start = max(t_enter - edge_thickness * 2.0, 0.0);
            let end = min(t_exit + edge_thickness * 2.0, max_dist);
            let overlay_steps = 128;
            let step_size = (end - start) / f32(overlay_steps);

            for (var j = 0; j < overlay_steps; j = j + 1) {
                let ot = start + step_size * (f32(j) + 0.5);

                // Don't draw overlay behind the SDF surface
                if (hit && ot > t) { break; }

                let op = globals.camera_pos + ray_dir * ot;

                // Check AABB edge
                if (globals.show_aabb != 0u) {
                    let d_aabb = box_frame_dist(op, globals.aabb_min, aabb_max, edge_thickness);
                    if (d_aabb < 0.0) {
                        let overlay_color = vec3<f32>(0.2, 0.8, 0.2);
                        color = mix(color, overlay_color, 0.6);
                        break;
                    }
                }

                // Check brick boundary
                if (globals.show_bricks != 0u && brick_world > 0.0) {
                    let d_brick = bricks_frame_dist(op, globals.aabb_min, globals.aabb_size, brick_world, edge_thickness * 0.5);
                    if (d_brick < 0.0) {
                        let overlay_color = vec3<f32>(0.3, 0.5, 0.9);
                        color = mix(color, overlay_color, 0.35);
                        break;
                    }
                }
            }
        }
    }

    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
