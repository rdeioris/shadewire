// Port of MaterialX libraries/stdlib/genglsl/mx_rotate_vector2.glsl (Apache-2.0, see NOTICE).
// Mirrors crates/shadewire/src/ops/vector.rs.

// `deg` in degrees; a positive amount turns +x towards -y.
vec2 shadewire_rotate_vector2(vec2 v, float deg) {
    float angle = deg * (3.14159265358979323846 / 180.0);
    float sa = sin(angle);
    float ca = cos(angle);
    return vec2(ca * v.x + sa * v.y, -sa * v.x + ca * v.y);
}
