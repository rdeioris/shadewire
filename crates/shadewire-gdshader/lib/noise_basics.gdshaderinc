// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/noise/basics.rs.

// mx_select (Rust select)
float shadewire_select(bool b, float t, float f) {
    return b ? t : f;
}

// mx_negate_if (Rust negate_if)
float shadewire_negate_if(float val, bool b) {
    return b ? -val : val;
}

// mx_floor (Rust floor). int(float) rounds toward zero like Rust's `as i32`; the two agree
// for |x| < 2^31.
int shadewire_floor(float x) {
    return int(floor(x));
}

// mx_floorfrac (Rust floorfrac, which returns the pair): the fraction, the floor through `i`.
float shadewire_floorfrac(float x, out int i) {
    i = shadewire_floor(x);
    return x - float(i);
}

// mx_bilerp(float) (Rust bilerp_float)
float shadewire_bilerp_float(float v0, float v1, float v2, float v3, float s, float t) {
    float s1 = 1.0 - s;
    return (1.0 - t) * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s);
}

// mx_bilerp(vec3) (Rust bilerp_vec3)
vec3 shadewire_bilerp_vec3(vec3 v0, vec3 v1, vec3 v2, vec3 v3, float s, float t) {
    float s1 = 1.0 - s;
    return (1.0 - t) * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s);
}

// mx_trilerp(float) (Rust trilerp_float)
float shadewire_trilerp_float(float v0, float v1, float v2, float v3, float v4, float v5, float v6, float v7, float s, float t, float r) {
    float s1 = 1.0 - s;
    float t1 = 1.0 - t;
    float r1 = 1.0 - r;
    return r1 * (t1 * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s)) + r * (t1 * (v4 * s1 + v5 * s) + t * (v6 * s1 + v7 * s));
}

// mx_trilerp(vec3) (Rust trilerp_vec3)
vec3 shadewire_trilerp_vec3(vec3 v0, vec3 v1, vec3 v2, vec3 v3, vec3 v4, vec3 v5, vec3 v6, vec3 v7, float s, float t, float r) {
    float s1 = 1.0 - s;
    float t1 = 1.0 - t;
    float r1 = 1.0 - r;
    return r1 * (t1 * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s)) + r * (t1 * (v4 * s1 + v5 * s) + t * (v6 * s1 + v7 * s));
}
