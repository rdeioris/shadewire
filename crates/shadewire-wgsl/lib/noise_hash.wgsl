// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/noise/hash.rs.
// Bob Jenkins lookup3. WGSL u32 arithmetic wraps like Rust's wrapping_*; bitcast<u32> of an i32
// is Rust's `as u32` (the same bits).

// mx_rotl32 (Rust rotl32)
fn mx_rotl32(x: u32, k: i32) -> u32 {
    return (x << u32(k)) | (x >> (32u - u32(k)));
}

// mx_bjmix (Rust bjmix)
fn mx_bjmix(a: ptr<function, u32>, b: ptr<function, u32>, c: ptr<function, u32>) {
    *a -= *c; *a ^= mx_rotl32(*c, 4);  *c += *b;
    *b -= *a; *b ^= mx_rotl32(*a, 6);  *a += *c;
    *c -= *b; *c ^= mx_rotl32(*b, 8);  *b += *a;
    *a -= *c; *a ^= mx_rotl32(*c, 16); *c += *b;
    *b -= *a; *b ^= mx_rotl32(*a, 19); *a += *c;
    *c -= *b; *c ^= mx_rotl32(*b, 4);  *b += *a;
}

// mx_bjfinal (Rust bjfinal)
fn mx_bjfinal(a_in: u32, b_in: u32, c_in: u32) -> u32 {
    var a = a_in;
    var b = b_in;
    var c = c_in;
    c ^= b; c -= mx_rotl32(b, 14);
    a ^= c; a -= mx_rotl32(c, 11);
    b ^= a; b -= mx_rotl32(a, 25);
    c ^= b; c -= mx_rotl32(b, 16);
    a ^= c; a -= mx_rotl32(c, 4);
    b ^= a; b -= mx_rotl32(a, 14);
    c ^= b; c -= mx_rotl32(b, 24);
    return c;
}

// mx_bits_to_01 (Rust bits_to_01)
fn mx_bits_to_01(bits: u32) -> f32 {
    return f32(bits) / f32(0xffffffffu);
}

// mx_hash_int(int) (Rust hash_int_1)
fn mx_hash_int_1(x: i32) -> u32 {
    let len = 1u;
    let s = 0xdeadbeefu + (len << 2u) + 13u;
    return mx_bjfinal(s + bitcast<u32>(x), s, s);
}

// mx_hash_int(int, int) (Rust hash_int_2)
fn mx_hash_int_2(x: i32, y: i32) -> u32 {
    let len = 2u;
    let s = 0xdeadbeefu + (len << 2u) + 13u;
    let a = s + bitcast<u32>(x);
    let b = s + bitcast<u32>(y);
    return mx_bjfinal(a, b, s);
}

// mx_hash_int(int, int, int) (Rust hash_int_3)
fn mx_hash_int_3(x: i32, y: i32, z: i32) -> u32 {
    let len = 3u;
    let s = 0xdeadbeefu + (len << 2u) + 13u;
    let a = s + bitcast<u32>(x);
    let b = s + bitcast<u32>(y);
    let c = s + bitcast<u32>(z);
    return mx_bjfinal(a, b, c);
}

// mx_hash_int(int, int, int, int) (Rust hash_int_4)
fn mx_hash_int_4(x: i32, y: i32, z: i32, xx: i32) -> u32 {
    let len = 4u;
    let s = 0xdeadbeefu + (len << 2u) + 13u;
    var a = s + bitcast<u32>(x);
    var b = s + bitcast<u32>(y);
    var c = s + bitcast<u32>(z);
    mx_bjmix(&a, &b, &c);
    a += bitcast<u32>(xx);
    return mx_bjfinal(a, b, c);
}

// mx_hash_int(int, int, int, int, int) (Rust hash_int_5)
fn mx_hash_int_5(x: i32, y: i32, z: i32, xx: i32, yy: i32) -> u32 {
    let len = 5u;
    let s = 0xdeadbeefu + (len << 2u) + 13u;
    var a = s + bitcast<u32>(x);
    var b = s + bitcast<u32>(y);
    var c = s + bitcast<u32>(z);
    mx_bjmix(&a, &b, &c);
    a += bitcast<u32>(xx);
    b += bitcast<u32>(yy);
    return mx_bjfinal(a, b, c);
}

// mx_hash_vec3(int, int) (Rust hash_vec3_2)
fn mx_hash_vec3_2(x: i32, y: i32) -> vec3<u32> {
    let h = mx_hash_int_2(x, y);
    return vec3<u32>(h & 0xFFu, (h >> 8u) & 0xFFu, (h >> 16u) & 0xFFu);
}

// mx_hash_vec3(int, int, int) (Rust hash_vec3_3)
fn mx_hash_vec3_3(x: i32, y: i32, z: i32) -> vec3<u32> {
    let h = mx_hash_int_3(x, y, z);
    return vec3<u32>(h & 0xFFu, (h >> 8u) & 0xFFu, (h >> 16u) & 0xFFu);
}
