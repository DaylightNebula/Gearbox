@fragment
fn fs_final() -> @location(0) vec4<f32> {
    let effect_color = vec3<f32>(1.0, 0.5, 0.2); // e.g. a tint or glow
    let strength = 0.3; // your "how much to blend in" value
    return vec4<f32>(effect_color, strength);
}
