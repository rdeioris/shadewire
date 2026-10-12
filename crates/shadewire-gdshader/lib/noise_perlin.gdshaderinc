// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/noise/perlin.rs.
// Needs noise_basics and noise_hash. int `+` wraps like Rust's wrapping_add.

// mx_gradient_float(uint, float, float) (Rust gradient_float_2d)
float shadewire_gradient_float_2d(uint hash, float x, float y) {
    // 8 possible directions (+-1,+-2) and (+-2,+-1)
    uint h = hash & 7u;
    float u = shadewire_select(h < 4u, x, y);
    float v = 2.0 * shadewire_select(h < 4u, y, x);
    return shadewire_negate_if(u, (h & 1u) != 0u) + shadewire_negate_if(v, (h & 2u) != 0u);
}

// mx_gradient_float(uint, float, float, float) (Rust gradient_float_3d)
float shadewire_gradient_float_3d(uint hash, float x, float y, float z) {
    // use vectors pointing to the edges of the cube
    uint h = hash & 15u;
    float u = shadewire_select(h < 8u, x, y);
    float v = shadewire_select(h < 4u, y, shadewire_select((h == 12u) || (h == 14u), x, z));
    return shadewire_negate_if(u, (h & 1u) != 0u) + shadewire_negate_if(v, (h & 2u) != 0u);
}

// mx_gradient_vec3(uvec3, float, float) (Rust gradient_vec3_2d)
vec3 shadewire_gradient_vec3_2d(uvec3 hash, float x, float y) {
    return vec3(shadewire_gradient_float_2d(hash.x, x, y), shadewire_gradient_float_2d(hash.y, x, y), shadewire_gradient_float_2d(hash.z, x, y));
}

// mx_gradient_vec3(uvec3, float, float, float) (Rust gradient_vec3_3d)
vec3 shadewire_gradient_vec3_3d(uvec3 hash, float x, float y, float z) {
    return vec3(shadewire_gradient_float_3d(hash.x, x, y, z), shadewire_gradient_float_3d(hash.y, x, y, z), shadewire_gradient_float_3d(hash.z, x, y, z));
}

// mx_gradient_scale2d(float) (Rust gradient_scale2d_float)
float shadewire_gradient_scale2d_float(float v) {
    return 0.6616 * v;
}

// mx_gradient_scale3d(float) (Rust gradient_scale3d_float)
float shadewire_gradient_scale3d_float(float v) {
    return 0.9820 * v;
}

// mx_gradient_scale2d(vec3) (Rust gradient_scale2d_vec3)
vec3 shadewire_gradient_scale2d_vec3(vec3 v) {
    return 0.6616 * v;
}

// mx_gradient_scale3d(vec3) (Rust gradient_scale3d_vec3)
vec3 shadewire_gradient_scale3d_vec3(vec3 v) {
    return 0.9820 * v;
}

// mx_fade (Rust fade)
float shadewire_fade(float t) {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

// mx_perlin_noise_float(vec2) (Rust perlin_noise_float_2d)
float shadewire_perlin_noise_float_2d(vec2 p) {
    int x = 0;
    int y = 0;
    float fx = shadewire_floorfrac(p.x, x);
    float fy = shadewire_floorfrac(p.y, y);
    float u = shadewire_fade(fx);
    float v = shadewire_fade(fy);
    int x1 = x + 1;
    int y1 = y + 1;
    float result = shadewire_bilerp_float(
        shadewire_gradient_float_2d(shadewire_hash_int_2(x, y), fx, fy),
        shadewire_gradient_float_2d(shadewire_hash_int_2(x1, y), fx - 1.0, fy),
        shadewire_gradient_float_2d(shadewire_hash_int_2(x, y1), fx, fy - 1.0),
        shadewire_gradient_float_2d(shadewire_hash_int_2(x1, y1), fx - 1.0, fy - 1.0),
        u,
        v);
    return shadewire_gradient_scale2d_float(result);
}

// mx_perlin_noise_float(vec3) (Rust perlin_noise_float_3d)
float shadewire_perlin_noise_float_3d(vec3 p) {
    int x = 0;
    int y = 0;
    int z = 0;
    float fx = shadewire_floorfrac(p.x, x);
    float fy = shadewire_floorfrac(p.y, y);
    float fz = shadewire_floorfrac(p.z, z);
    float u = shadewire_fade(fx);
    float v = shadewire_fade(fy);
    float w = shadewire_fade(fz);
    int x1 = x + 1;
    int y1 = y + 1;
    int z1 = z + 1;
    float result = shadewire_trilerp_float(
        shadewire_gradient_float_3d(shadewire_hash_int_3(x, y, z), fx, fy, fz),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x1, y, z), fx - 1.0, fy, fz),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x, y1, z), fx, fy - 1.0, fz),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x1, y1, z), fx - 1.0, fy - 1.0, fz),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x, y, z1), fx, fy, fz - 1.0),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x1, y, z1), fx - 1.0, fy, fz - 1.0),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x, y1, z1), fx, fy - 1.0, fz - 1.0),
        shadewire_gradient_float_3d(shadewire_hash_int_3(x1, y1, z1), fx - 1.0, fy - 1.0, fz - 1.0),
        u,
        v,
        w);
    return shadewire_gradient_scale3d_float(result);
}

// mx_perlin_noise_vec3(vec2) (Rust perlin_noise_vec3_2d)
vec3 shadewire_perlin_noise_vec3_2d(vec2 p) {
    int x = 0;
    int y = 0;
    float fx = shadewire_floorfrac(p.x, x);
    float fy = shadewire_floorfrac(p.y, y);
    float u = shadewire_fade(fx);
    float v = shadewire_fade(fy);
    int x1 = x + 1;
    int y1 = y + 1;
    vec3 result = shadewire_bilerp_vec3(
        shadewire_gradient_vec3_2d(shadewire_hash_vec3_2(x, y), fx, fy),
        shadewire_gradient_vec3_2d(shadewire_hash_vec3_2(x1, y), fx - 1.0, fy),
        shadewire_gradient_vec3_2d(shadewire_hash_vec3_2(x, y1), fx, fy - 1.0),
        shadewire_gradient_vec3_2d(shadewire_hash_vec3_2(x1, y1), fx - 1.0, fy - 1.0),
        u,
        v);
    return shadewire_gradient_scale2d_vec3(result);
}

// mx_perlin_noise_vec3(vec3) (Rust perlin_noise_vec3_3d)
vec3 shadewire_perlin_noise_vec3_3d(vec3 p) {
    int x = 0;
    int y = 0;
    int z = 0;
    float fx = shadewire_floorfrac(p.x, x);
    float fy = shadewire_floorfrac(p.y, y);
    float fz = shadewire_floorfrac(p.z, z);
    float u = shadewire_fade(fx);
    float v = shadewire_fade(fy);
    float w = shadewire_fade(fz);
    int x1 = x + 1;
    int y1 = y + 1;
    int z1 = z + 1;
    vec3 result = shadewire_trilerp_vec3(
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x, y, z), fx, fy, fz),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x1, y, z), fx - 1.0, fy, fz),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x, y1, z), fx, fy - 1.0, fz),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x1, y1, z), fx - 1.0, fy - 1.0, fz),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x, y, z1), fx, fy, fz - 1.0),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x1, y, z1), fx - 1.0, fy, fz - 1.0),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x, y1, z1), fx, fy - 1.0, fz - 1.0),
        shadewire_gradient_vec3_3d(shadewire_hash_vec3_3(x1, y1, z1), fx - 1.0, fy - 1.0, fz - 1.0),
        u,
        v,
        w);
    return shadewire_gradient_scale3d_vec3(result);
}
