// Port of MaterialX libraries/stdlib/genglsl/mx_rotate_vector2.glsl (Apache-2.0, see NOTICE).
// Mirrors crates/shadewire-ir/src/ops/vector.rs.

// `amount` in degrees; a positive amount turns +x towards -y.
fn shadewire_rotate_vector2(v: vec2<f32>, amount: f32) -> vec2<f32> {
    let radians = amount * (3.14159265358979323846 / 180.0);
    let sa = sin(radians);
    let ca = cos(radians);
    return vec2<f32>(ca * v.x + sa * v.y, -sa * v.x + ca * v.y);
}
