// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/noise/perlin.rs.
// Needs noise_basics and noise_hash. WGSL i32 `+` wraps like Rust's wrapping_add.

// mx_gradient_float(uint, float, float) (Rust gradient_float_2d)
fn mx_gradient_float_2d(hash: u32, x: f32, y: f32) -> f32 {
    // 8 possible directions (+-1,+-2) and (+-2,+-1)
    let h = hash & 7u;
    let u = mx_select(h < 4u, x, y);
    let v = 2.0 * mx_select(h < 4u, y, x);
    return mx_negate_if(u, (h & 1u) != 0u) + mx_negate_if(v, (h & 2u) != 0u);
}

// mx_gradient_float(uint, float, float, float) (Rust gradient_float_3d)
fn mx_gradient_float_3d(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    // use vectors pointing to the edges of the cube
    let h = hash & 15u;
    let u = mx_select(h < 8u, x, y);
    let v = mx_select(h < 4u, y, mx_select((h == 12u) || (h == 14u), x, z));
    return mx_negate_if(u, (h & 1u) != 0u) + mx_negate_if(v, (h & 2u) != 0u);
}

// mx_gradient_vec3(uvec3, float, float) (Rust gradient_vec3_2d)
fn mx_gradient_vec3_2d(hash: vec3<u32>, x: f32, y: f32) -> vec3<f32> {
    return vec3<f32>(mx_gradient_float_2d(hash.x, x, y), mx_gradient_float_2d(hash.y, x, y), mx_gradient_float_2d(hash.z, x, y));
}

// mx_gradient_vec3(uvec3, float, float, float) (Rust gradient_vec3_3d)
fn mx_gradient_vec3_3d(hash: vec3<u32>, x: f32, y: f32, z: f32) -> vec3<f32> {
    return vec3<f32>(mx_gradient_float_3d(hash.x, x, y, z), mx_gradient_float_3d(hash.y, x, y, z), mx_gradient_float_3d(hash.z, x, y, z));
}

// mx_gradient_scale2d(float) (Rust gradient_scale2d_float)
fn mx_gradient_scale2d_float(v: f32) -> f32 {
    return 0.6616 * v;
}

// mx_gradient_scale3d(float) (Rust gradient_scale3d_float)
fn mx_gradient_scale3d_float(v: f32) -> f32 {
    return 0.9820 * v;
}

// mx_gradient_scale2d(vec3) (Rust gradient_scale2d_vec3)
fn mx_gradient_scale2d_vec3(v: vec3<f32>) -> vec3<f32> {
    return 0.6616 * v;
}

// mx_gradient_scale3d(vec3) (Rust gradient_scale3d_vec3)
fn mx_gradient_scale3d_vec3(v: vec3<f32>) -> vec3<f32> {
    return 0.9820 * v;
}

// mx_fade (Rust fade)
fn mx_fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

// mx_perlin_noise_float(vec2) (Rust perlin_noise_float_2d)
fn mx_perlin_noise_float_2d(p: vec2<f32>) -> f32 {
    var x: i32;
    var y: i32;
    let fx = mx_floorfrac(p.x, &x);
    let fy = mx_floorfrac(p.y, &y);
    let u = mx_fade(fx);
    let v = mx_fade(fy);
    let x1 = x + 1;
    let y1 = y + 1;
    let result = mx_bilerp_float(
        mx_gradient_float_2d(mx_hash_int_2(x, y), fx, fy),
        mx_gradient_float_2d(mx_hash_int_2(x1, y), fx - 1.0, fy),
        mx_gradient_float_2d(mx_hash_int_2(x, y1), fx, fy - 1.0),
        mx_gradient_float_2d(mx_hash_int_2(x1, y1), fx - 1.0, fy - 1.0),
        u,
        v,
    );
    return mx_gradient_scale2d_float(result);
}

// mx_perlin_noise_float(vec3) (Rust perlin_noise_float_3d)
fn mx_perlin_noise_float_3d(p: vec3<f32>) -> f32 {
    var x: i32;
    var y: i32;
    var z: i32;
    let fx = mx_floorfrac(p.x, &x);
    let fy = mx_floorfrac(p.y, &y);
    let fz = mx_floorfrac(p.z, &z);
    let u = mx_fade(fx);
    let v = mx_fade(fy);
    let w = mx_fade(fz);
    let x1 = x + 1;
    let y1 = y + 1;
    let z1 = z + 1;
    let result = mx_trilerp_float(
        mx_gradient_float_3d(mx_hash_int_3(x, y, z), fx, fy, fz),
        mx_gradient_float_3d(mx_hash_int_3(x1, y, z), fx - 1.0, fy, fz),
        mx_gradient_float_3d(mx_hash_int_3(x, y1, z), fx, fy - 1.0, fz),
        mx_gradient_float_3d(mx_hash_int_3(x1, y1, z), fx - 1.0, fy - 1.0, fz),
        mx_gradient_float_3d(mx_hash_int_3(x, y, z1), fx, fy, fz - 1.0),
        mx_gradient_float_3d(mx_hash_int_3(x1, y, z1), fx - 1.0, fy, fz - 1.0),
        mx_gradient_float_3d(mx_hash_int_3(x, y1, z1), fx, fy - 1.0, fz - 1.0),
        mx_gradient_float_3d(mx_hash_int_3(x1, y1, z1), fx - 1.0, fy - 1.0, fz - 1.0),
        u,
        v,
        w,
    );
    return mx_gradient_scale3d_float(result);
}

// mx_perlin_noise_vec3(vec2) (Rust perlin_noise_vec3_2d)
fn mx_perlin_noise_vec3_2d(p: vec2<f32>) -> vec3<f32> {
    var x: i32;
    var y: i32;
    let fx = mx_floorfrac(p.x, &x);
    let fy = mx_floorfrac(p.y, &y);
    let u = mx_fade(fx);
    let v = mx_fade(fy);
    let x1 = x + 1;
    let y1 = y + 1;
    let result = mx_bilerp_vec3(
        mx_gradient_vec3_2d(mx_hash_vec3_2(x, y), fx, fy),
        mx_gradient_vec3_2d(mx_hash_vec3_2(x1, y), fx - 1.0, fy),
        mx_gradient_vec3_2d(mx_hash_vec3_2(x, y1), fx, fy - 1.0),
        mx_gradient_vec3_2d(mx_hash_vec3_2(x1, y1), fx - 1.0, fy - 1.0),
        u,
        v,
    );
    return mx_gradient_scale2d_vec3(result);
}

// mx_perlin_noise_vec3(vec3) (Rust perlin_noise_vec3_3d)
fn mx_perlin_noise_vec3_3d(p: vec3<f32>) -> vec3<f32> {
    var x: i32;
    var y: i32;
    var z: i32;
    let fx = mx_floorfrac(p.x, &x);
    let fy = mx_floorfrac(p.y, &y);
    let fz = mx_floorfrac(p.z, &z);
    let u = mx_fade(fx);
    let v = mx_fade(fy);
    let w = mx_fade(fz);
    let x1 = x + 1;
    let y1 = y + 1;
    let z1 = z + 1;
    let result = mx_trilerp_vec3(
        mx_gradient_vec3_3d(mx_hash_vec3_3(x, y, z), fx, fy, fz),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x1, y, z), fx - 1.0, fy, fz),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x, y1, z), fx, fy - 1.0, fz),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x1, y1, z), fx - 1.0, fy - 1.0, fz),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x, y, z1), fx, fy, fz - 1.0),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x1, y, z1), fx - 1.0, fy, fz - 1.0),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x, y1, z1), fx, fy - 1.0, fz - 1.0),
        mx_gradient_vec3_3d(mx_hash_vec3_3(x1, y1, z1), fx - 1.0, fy - 1.0, fz - 1.0),
        u,
        v,
        w,
    );
    return mx_gradient_scale3d_vec3(result);
}
