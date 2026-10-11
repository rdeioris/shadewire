//! Cell noise.
//! Ported from MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0), itself derived
//! from OSL noise, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc., et al., BSD-3-Clause.
//! See NOTICE.

use super::basics::floor;
use super::hash::{bits_to_01, bjfinal, bjmix, hash_int_1, hash_int_2, hash_int_3, hash_int_4};

// mx_cell_noise_float(float)
pub fn cell_noise_float_1d(p: f32) -> f32 {
    let ix = floor(p);
    bits_to_01(hash_int_1(ix))
}

// mx_cell_noise_float(vec2)
pub fn cell_noise_float_2d(p: [f32; 2]) -> f32 {
    let ix = floor(p[0]);
    let iy = floor(p[1]);
    bits_to_01(hash_int_2(ix, iy))
}

// mx_cell_noise_float(vec3)
pub fn cell_noise_float_3d(p: [f32; 3]) -> f32 {
    let ix = floor(p[0]);
    let iy = floor(p[1]);
    let iz = floor(p[2]);
    bits_to_01(hash_int_3(ix, iy, iz))
}

// mx_cell_noise_float(vec4)
pub fn cell_noise_float_4d(p: [f32; 4]) -> f32 {
    let ix = floor(p[0]);
    let iy = floor(p[1]);
    let iz = floor(p[2]);
    let iw = floor(p[3]);
    bits_to_01(hash_int_4(ix, iy, iz, iw))
}

// mx_cell_noise_vec3(float)
pub fn cell_noise_vec3_1d(p: f32) -> [f32; 3] {
    let ix = floor(p);
    [bits_to_01(hash_int_2(ix, 0)), bits_to_01(hash_int_2(ix, 1)), bits_to_01(hash_int_2(ix, 2))]
}

// mx_cell_noise_vec3(vec2)
pub fn cell_noise_vec3_2d(p: [f32; 2]) -> [f32; 3] {
    let ix = floor(p[0]);
    let iy = floor(p[1]);
    [bits_to_01(hash_int_3(ix, iy, 0)), bits_to_01(hash_int_3(ix, iy, 1)), bits_to_01(hash_int_3(ix, iy, 2))]
}

// mx_cell_noise_vec3(vec3)
pub fn cell_noise_vec3_3d(p: [f32; 3]) -> [f32; 3] {
    let ix = floor(p[0]);
    let iy = floor(p[1]);
    let iz = floor(p[2]);
    let s = 0xdead_beef_u32.wrapping_add(4 << 2).wrapping_add(13);
    let mut a = s.wrapping_add(ix as u32);
    let mut b = s.wrapping_add(iy as u32);
    let mut c = s.wrapping_add(iz as u32);
    bjmix(&mut a, &mut b, &mut c);
    [
        bits_to_01(bjfinal(a, b, c)),
        bits_to_01(bjfinal(a.wrapping_add(1), b, c)),
        bits_to_01(bjfinal(a.wrapping_add(2), b, c)),
    ]
}

// mx_cell_noise_vec3(vec4)
pub fn cell_noise_vec3_4d(p: [f32; 4]) -> [f32; 3] {
    let ix = floor(p[0]);
    let iy = floor(p[1]);
    let iz = floor(p[2]);
    let iw = floor(p[3]);
    let s = 0xdead_beef_u32.wrapping_add(5 << 2).wrapping_add(13);
    let mut a = s.wrapping_add(ix as u32);
    let mut b = s.wrapping_add(iy as u32);
    let mut c = s.wrapping_add(iz as u32);
    bjmix(&mut a, &mut b, &mut c);
    a = a.wrapping_add(iw as u32);
    [
        bits_to_01(bjfinal(a, b, c)),
        bits_to_01(bjfinal(a, b.wrapping_add(1), c)),
        bits_to_01(bjfinal(a, b.wrapping_add(2), c)),
    ]
}
