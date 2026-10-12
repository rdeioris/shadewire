// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/noise/cell.rs.
// Needs noise_basics and noise_hash.

// mx_cell_noise_float(float) (Rust cell_noise_float_1d)
fn shadewire_cell_noise_float_1d(p: f32) -> f32 {
    let ix = shadewire_floor(p);
    return shadewire_bits_to_01(shadewire_hash_int_1(ix));
}

// mx_cell_noise_float(vec2) (Rust cell_noise_float_2d)
fn shadewire_cell_noise_float_2d(p: vec2<f32>) -> f32 {
    let ix = shadewire_floor(p.x);
    let iy = shadewire_floor(p.y);
    return shadewire_bits_to_01(shadewire_hash_int_2(ix, iy));
}

// mx_cell_noise_float(vec3) (Rust cell_noise_float_3d)
fn shadewire_cell_noise_float_3d(p: vec3<f32>) -> f32 {
    let ix = shadewire_floor(p.x);
    let iy = shadewire_floor(p.y);
    let iz = shadewire_floor(p.z);
    return shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, iz));
}

// mx_cell_noise_float(vec4) (Rust cell_noise_float_4d)
fn shadewire_cell_noise_float_4d(p: vec4<f32>) -> f32 {
    let ix = shadewire_floor(p.x);
    let iy = shadewire_floor(p.y);
    let iz = shadewire_floor(p.z);
    let iw = shadewire_floor(p.w);
    return shadewire_bits_to_01(shadewire_hash_int_4(ix, iy, iz, iw));
}

// mx_cell_noise_vec3(float) (Rust cell_noise_vec3_1d)
fn shadewire_cell_noise_vec3_1d(p: f32) -> vec3<f32> {
    let ix = shadewire_floor(p);
    return vec3<f32>(
        shadewire_bits_to_01(shadewire_hash_int_2(ix, 0)),
        shadewire_bits_to_01(shadewire_hash_int_2(ix, 1)),
        shadewire_bits_to_01(shadewire_hash_int_2(ix, 2)),
    );
}

// mx_cell_noise_vec3(vec2) (Rust cell_noise_vec3_2d)
fn shadewire_cell_noise_vec3_2d(p: vec2<f32>) -> vec3<f32> {
    let ix = shadewire_floor(p.x);
    let iy = shadewire_floor(p.y);
    return vec3<f32>(
        shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, 0)),
        shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, 1)),
        shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, 2)),
    );
}

// mx_cell_noise_vec3(vec3) (Rust cell_noise_vec3_3d)
fn shadewire_cell_noise_vec3_3d(p: vec3<f32>) -> vec3<f32> {
    let ix = shadewire_floor(p.x);
    let iy = shadewire_floor(p.y);
    let iz = shadewire_floor(p.z);
    let s = 0xdeadbeefu + (4u << 2u) + 13u;
    var a = s + bitcast<u32>(ix);
    var b = s + bitcast<u32>(iy);
    var c = s + bitcast<u32>(iz);
    shadewire_bjmix(&a, &b, &c);
    return vec3<f32>(
        shadewire_bits_to_01(shadewire_bjfinal(a, b, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a + 1u, b, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a + 2u, b, c)),
    );
}

// mx_cell_noise_vec3(vec4) (Rust cell_noise_vec3_4d)
fn shadewire_cell_noise_vec3_4d(p: vec4<f32>) -> vec3<f32> {
    let ix = shadewire_floor(p.x);
    let iy = shadewire_floor(p.y);
    let iz = shadewire_floor(p.z);
    let iw = shadewire_floor(p.w);
    let s = 0xdeadbeefu + (5u << 2u) + 13u;
    var a = s + bitcast<u32>(ix);
    var b = s + bitcast<u32>(iy);
    var c = s + bitcast<u32>(iz);
    shadewire_bjmix(&a, &b, &c);
    a += bitcast<u32>(iw);
    return vec3<f32>(
        shadewire_bits_to_01(shadewire_bjfinal(a, b, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a, b + 1u, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a, b + 2u, c)),
    );
}
