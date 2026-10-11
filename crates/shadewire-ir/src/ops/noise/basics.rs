//! Helpers at the head of mx_noise.glsl.
//! Ported from MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0), itself derived
//! from OSL noise, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc., et al., BSD-3-Clause.
//! See NOTICE.

// mx_select
pub fn select(b: bool, t: f32, f: f32) -> f32 {
    if b { t } else { f }
}

// mx_negate_if
pub fn negate_if(val: f32, b: bool) -> f32 {
    if b { -val } else { val }
}

// mx_floor
// `as i32` saturates out-of-range values and maps NaN to 0, where GLSL int(floor(x)) is undefined.
pub fn floor(x: f32) -> i32 {
    x.floor() as i32
}

// mx_floorfrac: returns the fraction and the floor.
pub fn floorfrac(x: f32) -> (f32, i32) {
    let i = floor(x);
    (x - i as f32, i)
}

// mx_bilerp (float)
pub fn bilerp_float(v0: f32, v1: f32, v2: f32, v3: f32, s: f32, t: f32) -> f32 {
    let s1 = 1.0 - s;
    (1.0 - t) * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s)
}

// mx_bilerp (vec3)
pub fn bilerp_vec3(v0: [f32; 3], v1: [f32; 3], v2: [f32; 3], v3: [f32; 3], s: f32, t: f32) -> [f32; 3] {
    let s1 = 1.0 - s;
    let mut out = [0.0; 3];
    for c in 0..3 {
        out[c] = (1.0 - t) * (v0[c] * s1 + v1[c] * s) + t * (v2[c] * s1 + v3[c] * s);
    }
    out
}

// mx_trilerp (float)
#[allow(clippy::too_many_arguments)]
pub fn trilerp_float(
    v0: f32, v1: f32, v2: f32, v3: f32, v4: f32, v5: f32, v6: f32, v7: f32,
    s: f32, t: f32, r: f32,
) -> f32 {
    let s1 = 1.0 - s;
    let t1 = 1.0 - t;
    let r1 = 1.0 - r;
    r1 * (t1 * (v0 * s1 + v1 * s) + t * (v2 * s1 + v3 * s)) + r * (t1 * (v4 * s1 + v5 * s) + t * (v6 * s1 + v7 * s))
}

// mx_trilerp (vec3)
#[allow(clippy::too_many_arguments)]
pub fn trilerp_vec3(
    v0: [f32; 3], v1: [f32; 3], v2: [f32; 3], v3: [f32; 3], v4: [f32; 3], v5: [f32; 3], v6: [f32; 3], v7: [f32; 3],
    s: f32, t: f32, r: f32,
) -> [f32; 3] {
    let mut out = [0.0; 3];
    for c in 0..3 {
        out[c] = trilerp_float(v0[c], v1[c], v2[c], v3[c], v4[c], v5[c], v6[c], v7[c], s, t, r);
    }
    out
}
