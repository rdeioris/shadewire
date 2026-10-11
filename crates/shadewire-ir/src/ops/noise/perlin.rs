//! Gradient (Perlin) noise.
//! Ported from MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0), itself derived
//! from OSL noise, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc., et al., BSD-3-Clause.
//! See NOTICE.

use super::basics::{bilerp_float, bilerp_vec3, floorfrac, negate_if, select, trilerp_float, trilerp_vec3};
use super::hash::{hash_int_2, hash_int_3, hash_vec3_2, hash_vec3_3};

// mx_gradient_float(uint, float, float)
pub fn gradient_float_2d(hash: u32, x: f32, y: f32) -> f32 {
    // 8 possible directions (+-1,+-2) and (+-2,+-1)
    let h = hash & 7;
    let u = select(h < 4, x, y);
    let v = 2.0 * select(h < 4, y, x);
    negate_if(u, (h & 1) != 0) + negate_if(v, (h & 2) != 0)
}

// mx_gradient_float(uint, float, float, float)
pub fn gradient_float_3d(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    // use vectors pointing to the edges of the cube
    let h = hash & 15;
    let u = select(h < 8, x, y);
    let v = select(h < 4, y, select((h == 12) || (h == 14), x, z));
    negate_if(u, (h & 1) != 0) + negate_if(v, (h & 2) != 0)
}

// mx_gradient_vec3(uvec3, float, float)
pub fn gradient_vec3_2d(hash: [u32; 3], x: f32, y: f32) -> [f32; 3] {
    [gradient_float_2d(hash[0], x, y), gradient_float_2d(hash[1], x, y), gradient_float_2d(hash[2], x, y)]
}

// mx_gradient_vec3(uvec3, float, float, float)
pub fn gradient_vec3_3d(hash: [u32; 3], x: f32, y: f32, z: f32) -> [f32; 3] {
    [gradient_float_3d(hash[0], x, y, z), gradient_float_3d(hash[1], x, y, z), gradient_float_3d(hash[2], x, y, z)]
}

// mx_gradient_scale2d(float)
pub fn gradient_scale2d_float(v: f32) -> f32 {
    0.6616 * v
}

// mx_gradient_scale3d(float)
pub fn gradient_scale3d_float(v: f32) -> f32 {
    0.9820 * v
}

// mx_gradient_scale2d(vec3)
pub fn gradient_scale2d_vec3(v: [f32; 3]) -> [f32; 3] {
    [0.6616 * v[0], 0.6616 * v[1], 0.6616 * v[2]]
}

// mx_gradient_scale3d(vec3)
pub fn gradient_scale3d_vec3(v: [f32; 3]) -> [f32; 3] {
    [0.9820 * v[0], 0.9820 * v[1], 0.9820 * v[2]]
}

// mx_fade
pub fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

// mx_perlin_noise_float(vec2)
pub fn perlin_noise_float_2d(p: [f32; 2]) -> f32 {
    let (fx, x) = floorfrac(p[0]);
    let (fy, y) = floorfrac(p[1]);
    let u = fade(fx);
    let v = fade(fy);
    let (x1, y1) = (x.wrapping_add(1), y.wrapping_add(1));
    let result = bilerp_float(
        gradient_float_2d(hash_int_2(x, y), fx, fy),
        gradient_float_2d(hash_int_2(x1, y), fx - 1.0, fy),
        gradient_float_2d(hash_int_2(x, y1), fx, fy - 1.0),
        gradient_float_2d(hash_int_2(x1, y1), fx - 1.0, fy - 1.0),
        u,
        v,
    );
    gradient_scale2d_float(result)
}

// mx_perlin_noise_float(vec3)
pub fn perlin_noise_float_3d(p: [f32; 3]) -> f32 {
    let (fx, x) = floorfrac(p[0]);
    let (fy, y) = floorfrac(p[1]);
    let (fz, z) = floorfrac(p[2]);
    let u = fade(fx);
    let v = fade(fy);
    let w = fade(fz);
    let (x1, y1, z1) = (x.wrapping_add(1), y.wrapping_add(1), z.wrapping_add(1));
    let result = trilerp_float(
        gradient_float_3d(hash_int_3(x, y, z), fx, fy, fz),
        gradient_float_3d(hash_int_3(x1, y, z), fx - 1.0, fy, fz),
        gradient_float_3d(hash_int_3(x, y1, z), fx, fy - 1.0, fz),
        gradient_float_3d(hash_int_3(x1, y1, z), fx - 1.0, fy - 1.0, fz),
        gradient_float_3d(hash_int_3(x, y, z1), fx, fy, fz - 1.0),
        gradient_float_3d(hash_int_3(x1, y, z1), fx - 1.0, fy, fz - 1.0),
        gradient_float_3d(hash_int_3(x, y1, z1), fx, fy - 1.0, fz - 1.0),
        gradient_float_3d(hash_int_3(x1, y1, z1), fx - 1.0, fy - 1.0, fz - 1.0),
        u,
        v,
        w,
    );
    gradient_scale3d_float(result)
}

// mx_perlin_noise_vec3(vec2)
pub fn perlin_noise_vec3_2d(p: [f32; 2]) -> [f32; 3] {
    let (fx, x) = floorfrac(p[0]);
    let (fy, y) = floorfrac(p[1]);
    let u = fade(fx);
    let v = fade(fy);
    let (x1, y1) = (x.wrapping_add(1), y.wrapping_add(1));
    let result = bilerp_vec3(
        gradient_vec3_2d(hash_vec3_2(x, y), fx, fy),
        gradient_vec3_2d(hash_vec3_2(x1, y), fx - 1.0, fy),
        gradient_vec3_2d(hash_vec3_2(x, y1), fx, fy - 1.0),
        gradient_vec3_2d(hash_vec3_2(x1, y1), fx - 1.0, fy - 1.0),
        u,
        v,
    );
    gradient_scale2d_vec3(result)
}

// mx_perlin_noise_vec3(vec3)
pub fn perlin_noise_vec3_3d(p: [f32; 3]) -> [f32; 3] {
    let (fx, x) = floorfrac(p[0]);
    let (fy, y) = floorfrac(p[1]);
    let (fz, z) = floorfrac(p[2]);
    let u = fade(fx);
    let v = fade(fy);
    let w = fade(fz);
    let (x1, y1, z1) = (x.wrapping_add(1), y.wrapping_add(1), z.wrapping_add(1));
    let result = trilerp_vec3(
        gradient_vec3_3d(hash_vec3_3(x, y, z), fx, fy, fz),
        gradient_vec3_3d(hash_vec3_3(x1, y, z), fx - 1.0, fy, fz),
        gradient_vec3_3d(hash_vec3_3(x, y1, z), fx, fy - 1.0, fz),
        gradient_vec3_3d(hash_vec3_3(x1, y1, z), fx - 1.0, fy - 1.0, fz),
        gradient_vec3_3d(hash_vec3_3(x, y, z1), fx, fy, fz - 1.0),
        gradient_vec3_3d(hash_vec3_3(x1, y, z1), fx - 1.0, fy, fz - 1.0),
        gradient_vec3_3d(hash_vec3_3(x, y1, z1), fx, fy - 1.0, fz - 1.0),
        gradient_vec3_3d(hash_vec3_3(x1, y1, z1), fx - 1.0, fy - 1.0, fz - 1.0),
        u,
        v,
        w,
    );
    gradient_scale3d_vec3(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Traced by hand from mx_gradient_float(uint, float, float) at (x, y) = (1, 10).
    #[test]
    fn gradient_2d_picks_the_eight_directions() {
        let want = [21.0, 19.0, -19.0, -21.0, 12.0, -8.0, 8.0, -12.0];
        for (h, w) in want.iter().enumerate() {
            assert_eq!(gradient_float_2d(h as u32 + 8, 1.0, 10.0), *w, "hash {h}");
        }
    }

    // Traced by hand from mx_gradient_float(uint, float, float, float) at (1, 10, 100).
    #[test]
    fn gradient_3d_picks_the_cube_edges() {
        let want = [
            11.0, 9.0, -9.0, -11.0, 101.0, 99.0, -99.0, -101.0, 110.0, 90.0, -90.0, -110.0, 11.0, 90.0, 9.0, -110.0,
        ];
        for (h, w) in want.iter().enumerate() {
            assert_eq!(gradient_float_3d(h as u32 + 16, 1.0, 10.0, 100.0), *w, "hash {h}");
        }
    }

    #[test]
    fn fade_hits_its_ends_and_middle() {
        assert_eq!((fade(0.0), fade(0.5), fade(1.0)), (0.0, 0.5, 1.0));
        assert_eq!(fade(0.25), 53.0 / 512.0);
    }
}
