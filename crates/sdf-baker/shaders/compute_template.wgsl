// --- compute_template.wgsl ---
// System-generated compute shader for SDF evaluation.
// The user's sdf() function is inserted below by shader composition.

struct Params {
    aabb_min: vec3<f32>,
    voxel_size: f32,
    brick_offset: vec3<u32>,
    brick_size: u32,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read_write> output: array<f32>;

{{USER_SDF}}

@compute @workgroup_size(4, 4, 4)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let B = params.brick_size;
    if (gid.x >= B || gid.y >= B || gid.z >= B) { return; }

    let global_idx = vec3<u32>(
        params.brick_offset.x + gid.x,
        params.brick_offset.y + gid.y,
        params.brick_offset.z + gid.z,
    );

    let p = params.aabb_min + params.voxel_size * (vec3<f32>(global_idx) + vec3<f32>(0.5));
    let d = sdf(p);

    let idx = gid.x + B * (gid.y + B * gid.z);
    output[idx] = d;
}
