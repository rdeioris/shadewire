// Port of MaterialX libraries/stdlib/genglsl/mx_normalmap.glsl (Apache-2.0, see NOTICE).
// Mirrors crates/shadewire-ir/src/ops/normal.rs: the dot products and the normalize are
// written out in the interpreter's order.

// mx_normalmap_vector2 (the float variant splats its scale); a zero input means flat.
fn mx_normalmap(v: vec3<f32>, scale: vec2<f32>, n: vec3<f32>, t: vec3<f32>, b: vec3<f32>) -> vec3<f32> {
    var value = v * 2.0 - 1.0;
    if v.x * v.x + v.y * v.y + v.z * v.z == 0.0 {
        value = vec3<f32>(0.0, 0.0, 1.0);
    }
    let out = t * value.x * scale.x + b * value.y * scale.y + n * value.z;
    let len = sqrt(out.x * out.x + out.y * out.y + out.z * out.z);
    return out / len;
}
