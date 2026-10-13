// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/noise/basics.rs.

// mx_select (Rust select)
fn shadewire_select(b: bool, t: f32, f: f32) -> f32 {
    return select(f, t, b);
}

// mx_negate_if (Rust negate_if)
fn shadewire_negate_if(val: f32, b: bool) -> f32 {
    return select(val, -val, b);
}

// mx_floor (Rust floor). WGSL i32(f32) rounds toward zero like Rust's `as i32`; out of range it
// saturates to [-2^31, 2147483520] where Rust saturates to i32::MAX, and NaN has no defined
// result (Rust gives 0). The two agree for |x| < 2^31.
fn shadewire_floor(x: f32) -> i32 {
    return i32(floor(x));
}

// mx_floorfrac (Rust floorfrac, which returns the pair): the fraction, the floor through `i`.
fn shadewire_floorfrac(x: f32, i: ptr<function, i32>) -> f32 {
    *i = shadewire_floor(x);
    return x - f32(*i);
}

// mx_bilerp(float) (Rust bilerp_float)
fn shadewire_bilerp_float(v0: f32, v1: f32, v2: f32, v3: f32, s: f32, t: f32) -> f32 {
    let s1 = 1.0 - s;
    return (1.0 - t) * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s);
}

// mx_bilerp(vec3) (Rust bilerp_vec3)
fn shadewire_bilerp_vec3(v0: vec3<f32>, v1: vec3<f32>, v2: vec3<f32>, v3: vec3<f32>, s: f32, t: f32) -> vec3<f32> {
    let s1 = 1.0 - s;
    return (1.0 - t) * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s);
}

// mx_trilerp(float) (Rust trilerp_float)
fn shadewire_trilerp_float(v0: f32, v1: f32, v2: f32, v3: f32, v4: f32, v5: f32, v6: f32, v7: f32, s: f32, t: f32, r: f32) -> f32 {
    let s1 = 1.0 - s;
    let t1 = 1.0 - t;
    let r1 = 1.0 - r;
    return r1 * (t1 * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s)) + r * (t1 * (v4 * s1 + v5 * s) + t * (v6 * s1 + v7 * s));
}

// mx_trilerp(vec3) (Rust trilerp_vec3)
fn shadewire_trilerp_vec3(
    v0: vec3<f32>, v1: vec3<f32>, v2: vec3<f32>, v3: vec3<f32>, v4: vec3<f32>, v5: vec3<f32>, v6: vec3<f32>, v7: vec3<f32>,
    s: f32, t: f32, r: f32,
) -> vec3<f32> {
    let s1 = 1.0 - s;
    let t1 = 1.0 - t;
    let r1 = 1.0 - r;
    return r1 * (t1 * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s)) + r * (t1 * (v4 * s1 + v5 * s) + t * (v6 * s1 + v7 * s));
}
