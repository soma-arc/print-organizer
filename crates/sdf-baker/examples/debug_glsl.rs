use sdf_baker::shader_compose::{compose_shader, ShaderLang};
fn main() {
    let user = "float sdf(vec3 p) { return length(p - vec3(32.0)) - 25.6; }";
    let composed = compose_shader(ShaderLang::Glsl, user).unwrap();
    println!("Entry point: {}", composed.entry_point);
    println!("{}", composed.wgsl_source);
}
