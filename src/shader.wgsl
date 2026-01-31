// Vertex shader

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv : vec2<f32>,
};

struct Globals {
    // 16byte 境界に揃えるのが安全
    resolution: vec2<f32>,
    _pad: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> globals: Globals;

@vertex
fn vs_main(@builtin(vertex_index) vi : u32) -> VertexOutput {
    // 2 triangles = 6 vertices
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>(-1.0,  1.0),

        vec2<f32>(-1.0,  1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
    );

    let p = positions[vi];

    var out : VertexOutput;
    out.clip_position = vec4<f32>(p, 0.0, 1.0);
    // clip space (-1..1) -> uv (0..1)
    out.uv = p * 0.5 + vec2<f32>(0.5, 0.5);
    return out;
}

// Fragment shader

fn distCircle(uv: vec2<f32>, circle: vec3<f32>) -> f32 {
    let diff = uv - circle.xy;
    return length(diff) - circle.z;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    var uv = in.uv;
    uv = 2.0 * uv - vec2<f32>(1.0);
    uv = uv * vec2<f32>(globals.resolution.x / globals.resolution.y, 1.0);
    if(distCircle(uv, vec3<f32>(0.0, 0.0, 0.5)) < 0.0) {
        return vec4<f32>(1.0, 0.0, 0.0, 1.0);
    }
    return vec4<f32>(in.uv, 0.0, 1.0);
}