// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/noise/cell.rs.
// Needs noise_basics and noise_hash. 3735928559u is 0xdeadbeef.

// mx_cell_noise_float(float) (Rust cell_noise_float_1d)
float shadewire_cell_noise_float_1d(float p) {
    int ix = shadewire_floor(p);
    return shadewire_bits_to_01(shadewire_hash_int_1(ix));
}

// mx_cell_noise_float(vec2) (Rust cell_noise_float_2d)
float shadewire_cell_noise_float_2d(vec2 p) {
    int ix = shadewire_floor(p.x);
    int iy = shadewire_floor(p.y);
    return shadewire_bits_to_01(shadewire_hash_int_2(ix, iy));
}

// mx_cell_noise_float(vec3) (Rust cell_noise_float_3d)
float shadewire_cell_noise_float_3d(vec3 p) {
    int ix = shadewire_floor(p.x);
    int iy = shadewire_floor(p.y);
    int iz = shadewire_floor(p.z);
    return shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, iz));
}

// mx_cell_noise_float(vec4) (Rust cell_noise_float_4d)
float shadewire_cell_noise_float_4d(vec4 p) {
    int ix = shadewire_floor(p.x);
    int iy = shadewire_floor(p.y);
    int iz = shadewire_floor(p.z);
    int iw = shadewire_floor(p.w);
    return shadewire_bits_to_01(shadewire_hash_int_4(ix, iy, iz, iw));
}

// mx_cell_noise_vec3(float) (Rust cell_noise_vec3_1d)
vec3 shadewire_cell_noise_vec3_1d(float p) {
    int ix = shadewire_floor(p);
    return vec3(
        shadewire_bits_to_01(shadewire_hash_int_2(ix, 0)),
        shadewire_bits_to_01(shadewire_hash_int_2(ix, 1)),
        shadewire_bits_to_01(shadewire_hash_int_2(ix, 2)));
}

// mx_cell_noise_vec3(vec2) (Rust cell_noise_vec3_2d)
vec3 shadewire_cell_noise_vec3_2d(vec2 p) {
    int ix = shadewire_floor(p.x);
    int iy = shadewire_floor(p.y);
    return vec3(
        shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, 0)),
        shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, 1)),
        shadewire_bits_to_01(shadewire_hash_int_3(ix, iy, 2)));
}

// mx_cell_noise_vec3(vec3) (Rust cell_noise_vec3_3d)
vec3 shadewire_cell_noise_vec3_3d(vec3 p) {
    int ix = shadewire_floor(p.x);
    int iy = shadewire_floor(p.y);
    int iz = shadewire_floor(p.z);
    uint s = 3735928559u + (4u << 2u) + 13u;
    uint a = s + uint(ix);
    uint b = s + uint(iy);
    uint c = s + uint(iz);
    shadewire_bjmix(a, b, c);
    return vec3(
        shadewire_bits_to_01(shadewire_bjfinal(a, b, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a + 1u, b, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a + 2u, b, c)));
}

// mx_cell_noise_vec3(vec4) (Rust cell_noise_vec3_4d)
vec3 shadewire_cell_noise_vec3_4d(vec4 p) {
    int ix = shadewire_floor(p.x);
    int iy = shadewire_floor(p.y);
    int iz = shadewire_floor(p.z);
    int iw = shadewire_floor(p.w);
    uint s = 3735928559u + (5u << 2u) + 13u;
    uint a = s + uint(ix);
    uint b = s + uint(iy);
    uint c = s + uint(iz);
    shadewire_bjmix(a, b, c);
    a += uint(iw);
    return vec3(
        shadewire_bits_to_01(shadewire_bjfinal(a, b, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a, b + 1u, c)),
        shadewire_bits_to_01(shadewire_bjfinal(a, b + 2u, c)));
}
