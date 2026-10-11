//! Bob Jenkins lookup3 hashing.
//! Ported from MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0), itself derived
//! from OSL noise, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc., et al., BSD-3-Clause.
//! See NOTICE.

// mx_rotl32
pub fn rotl32(x: u32, k: i32) -> u32 {
    x.rotate_left(k as u32)
}

// mx_bjmix
pub fn bjmix(a: &mut u32, b: &mut u32, c: &mut u32) {
    *a = a.wrapping_sub(*c);
    *a ^= rotl32(*c, 4);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= rotl32(*a, 6);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= rotl32(*b, 8);
    *b = b.wrapping_add(*a);
    *a = a.wrapping_sub(*c);
    *a ^= rotl32(*c, 16);
    *c = c.wrapping_add(*b);
    *b = b.wrapping_sub(*a);
    *b ^= rotl32(*a, 19);
    *a = a.wrapping_add(*c);
    *c = c.wrapping_sub(*b);
    *c ^= rotl32(*b, 4);
    *b = b.wrapping_add(*a);
}

// mx_bjfinal
pub fn bjfinal(mut a: u32, mut b: u32, mut c: u32) -> u32 {
    c ^= b;
    c = c.wrapping_sub(rotl32(b, 14));
    a ^= c;
    a = a.wrapping_sub(rotl32(c, 11));
    b ^= a;
    b = b.wrapping_sub(rotl32(a, 25));
    c ^= b;
    c = c.wrapping_sub(rotl32(b, 16));
    a ^= c;
    a = a.wrapping_sub(rotl32(c, 4));
    b ^= a;
    b = b.wrapping_sub(rotl32(a, 14));
    c ^= b;
    c = c.wrapping_sub(rotl32(b, 24));
    c
}

// mx_bits_to_01
pub fn bits_to_01(bits: u32) -> f32 {
    bits as f32 / 0xffff_ffff_u32 as f32
}

// mx_hash_int(int)
pub fn hash_int_1(x: i32) -> u32 {
    let len: u32 = 1;
    let s = 0xdead_beef_u32.wrapping_add(len << 2).wrapping_add(13);
    bjfinal(s.wrapping_add(x as u32), s, s)
}

// mx_hash_int(int, int)
pub fn hash_int_2(x: i32, y: i32) -> u32 {
    let len: u32 = 2;
    let s = 0xdead_beef_u32.wrapping_add(len << 2).wrapping_add(13);
    let a = s.wrapping_add(x as u32);
    let b = s.wrapping_add(y as u32);
    bjfinal(a, b, s)
}

// mx_hash_int(int, int, int)
pub fn hash_int_3(x: i32, y: i32, z: i32) -> u32 {
    let len: u32 = 3;
    let s = 0xdead_beef_u32.wrapping_add(len << 2).wrapping_add(13);
    let a = s.wrapping_add(x as u32);
    let b = s.wrapping_add(y as u32);
    let c = s.wrapping_add(z as u32);
    bjfinal(a, b, c)
}

// mx_hash_int(int, int, int, int)
pub fn hash_int_4(x: i32, y: i32, z: i32, xx: i32) -> u32 {
    let len: u32 = 4;
    let s = 0xdead_beef_u32.wrapping_add(len << 2).wrapping_add(13);
    let mut a = s.wrapping_add(x as u32);
    let mut b = s.wrapping_add(y as u32);
    let mut c = s.wrapping_add(z as u32);
    bjmix(&mut a, &mut b, &mut c);
    a = a.wrapping_add(xx as u32);
    bjfinal(a, b, c)
}

// mx_hash_int(int, int, int, int, int)
pub fn hash_int_5(x: i32, y: i32, z: i32, xx: i32, yy: i32) -> u32 {
    let len: u32 = 5;
    let s = 0xdead_beef_u32.wrapping_add(len << 2).wrapping_add(13);
    let mut a = s.wrapping_add(x as u32);
    let mut b = s.wrapping_add(y as u32);
    let mut c = s.wrapping_add(z as u32);
    bjmix(&mut a, &mut b, &mut c);
    a = a.wrapping_add(xx as u32);
    b = b.wrapping_add(yy as u32);
    bjfinal(a, b, c)
}

// mx_hash_vec3(int, int)
pub fn hash_vec3_2(x: i32, y: i32) -> [u32; 3] {
    let h = hash_int_2(x, y);
    [h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF]
}

// mx_hash_vec3(int, int, int)
pub fn hash_vec3_3(x: i32, y: i32, z: i32) -> [u32; 3] {
    let h = hash_int_3(x, y, z);
    [h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_to_01_spans_the_unit_interval() {
        assert_eq!(bits_to_01(0), 0.0);
        assert_eq!(bits_to_01(u32::MAX), 1.0);
    }

    #[test]
    fn hash_vec3_splits_the_low_three_bytes() {
        let h = hash_int_2(5, -7);
        assert_eq!(hash_vec3_2(5, -7), [h & 0xFF, (h >> 8) & 0xFF, (h >> 16) & 0xFF]);
    }
}
