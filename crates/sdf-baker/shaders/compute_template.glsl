// --- compute_template.glsl ---
// System-generated compute shader for SDF evaluation (GLSL 450).
// The user's sdf() function is inserted below by shader composition.

#version 450

layout(local_size_x = 4, local_size_y = 4, local_size_z = 4) in;

layout(std140, set = 0, binding = 0) uniform Params {
    vec3 aabb_min;
    float voxel_size;
    uvec3 brick_offset;
    uint brick_size;
} params;

layout(std430, set = 0, binding = 1) buffer Output {
    float data[];
} output_buf;

{{USER_SDF}}

void main() {
    uvec3 gid = gl_GlobalInvocationID;
    uint B = params.brick_size;
    if (gid.x >= B || gid.y >= B || gid.z >= B) return;

    uvec3 global_idx = uvec3(
        params.brick_offset.x + gid.x,
        params.brick_offset.y + gid.y,
        params.brick_offset.z + gid.z
    );

    vec3 p = params.aabb_min + params.voxel_size * (vec3(global_idx) + vec3(0.5));
    float d = sdf(p);

    uint idx = gid.x + B * (gid.y + B * gid.z);
    output_buf.data[idx] = d;
}
