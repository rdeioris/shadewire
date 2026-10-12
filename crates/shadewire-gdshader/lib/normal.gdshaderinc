// Port of MaterialX libraries/stdlib/genglsl/mx_normalmap.glsl (Apache-2.0, see NOTICE).
// Mirrors crates/shadewire/src/ops/normal.rs: the dot products and the normalize are
// written out in the interpreter's order.

// mx_normalmap_vector2 (the float variant splats its scale); a zero input means flat.
vec3 shadewire_normalmap(vec3 v, vec2 nscale, vec3 n, vec3 t, vec3 b) {
    vec3 value = v * 2.0 - 1.0;
    if (v.x * v.x + v.y * v.y + v.z * v.z == 0.0) {
        value = vec3(0.0, 0.0, 1.0);
    }
    vec3 result = t * value.x * nscale.x + b * value.y * nscale.y + n * value.z;
    float len = sqrt(result.x * result.x + result.y * result.y + result.z * result.z);
    return result / len;
}
