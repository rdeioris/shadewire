// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/noise/hash.rs.
// Bob Jenkins lookup3. uint arithmetic wraps like Rust's wrapping_*; uint(int) keeps the bits,
// like Rust's `as u32`. 3735928559u is 0xdeadbeef.

// mx_rotl32 (Rust rotl32)
uint shadewire_rotl32(uint x, int k) {
    return (x << uint(k)) | (x >> (32u - uint(k)));
}

// mx_bjmix (Rust bjmix)
void shadewire_bjmix(inout uint a, inout uint b, inout uint c) {
    a -= c; a ^= shadewire_rotl32(c, 4);  c += b;
    b -= a; b ^= shadewire_rotl32(a, 6);  a += c;
    c -= b; c ^= shadewire_rotl32(b, 8);  b += a;
    a -= c; a ^= shadewire_rotl32(c, 16); c += b;
    b -= a; b ^= shadewire_rotl32(a, 19); a += c;
    c -= b; c ^= shadewire_rotl32(b, 4);  b += a;
}

// mx_bjfinal (Rust bjfinal)
uint shadewire_bjfinal(uint a_in, uint b_in, uint c_in) {
    uint a = a_in;
    uint b = b_in;
    uint c = c_in;
    c ^= b; c -= shadewire_rotl32(b, 14);
    a ^= c; a -= shadewire_rotl32(c, 11);
    b ^= a; b -= shadewire_rotl32(a, 25);
    c ^= b; c -= shadewire_rotl32(b, 16);
    a ^= c; a -= shadewire_rotl32(c, 4);
    b ^= a; b -= shadewire_rotl32(a, 14);
    c ^= b; c -= shadewire_rotl32(b, 24);
    return c;
}

// mx_bits_to_01 (Rust bits_to_01)
float shadewire_bits_to_01(uint bits) {
    return float(bits) / float(4294967295u);
}

// mx_hash_int(int) (Rust hash_int_1)
uint shadewire_hash_int_1(int x) {
    uint len = 1u;
    uint s = 3735928559u + (len << 2u) + 13u;
    return shadewire_bjfinal(s + uint(x), s, s);
}

// mx_hash_int(int, int) (Rust hash_int_2)
uint shadewire_hash_int_2(int x, int y) {
    uint len = 2u;
    uint s = 3735928559u + (len << 2u) + 13u;
    uint a = s + uint(x);
    uint b = s + uint(y);
    return shadewire_bjfinal(a, b, s);
}

// mx_hash_int(int, int, int) (Rust hash_int_3)
uint shadewire_hash_int_3(int x, int y, int z) {
    uint len = 3u;
    uint s = 3735928559u + (len << 2u) + 13u;
    uint a = s + uint(x);
    uint b = s + uint(y);
    uint c = s + uint(z);
    return shadewire_bjfinal(a, b, c);
}

// mx_hash_int(int, int, int, int) (Rust hash_int_4)
uint shadewire_hash_int_4(int x, int y, int z, int xx) {
    uint len = 4u;
    uint s = 3735928559u + (len << 2u) + 13u;
    uint a = s + uint(x);
    uint b = s + uint(y);
    uint c = s + uint(z);
    shadewire_bjmix(a, b, c);
    a += uint(xx);
    return shadewire_bjfinal(a, b, c);
}

// mx_hash_int(int, int, int, int, int) (Rust hash_int_5)
uint shadewire_hash_int_5(int x, int y, int z, int xx, int yy) {
    uint len = 5u;
    uint s = 3735928559u + (len << 2u) + 13u;
    uint a = s + uint(x);
    uint b = s + uint(y);
    uint c = s + uint(z);
    shadewire_bjmix(a, b, c);
    a += uint(xx);
    b += uint(yy);
    return shadewire_bjfinal(a, b, c);
}

// mx_hash_vec3(int, int) (Rust hash_vec3_2)
uvec3 shadewire_hash_vec3_2(int x, int y) {
    uint h = shadewire_hash_int_2(x, y);
    return uvec3(h & 255u, (h >> 8u) & 255u, (h >> 16u) & 255u);
}

// mx_hash_vec3(int, int, int) (Rust hash_vec3_3)
uvec3 shadewire_hash_vec3_3(int x, int y, int z) {
    uint h = shadewire_hash_int_3(x, y, z);
    return uvec3(h & 255u, (h >> 8u) & 255u, (h >> 16u) & 255u);
}
