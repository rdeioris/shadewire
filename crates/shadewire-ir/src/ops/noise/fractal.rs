//! Fractal (fBm) noise from genglsl/lib/mx_noise.glsl (Apache-2.0, ported from MaterialX).

use super::perlin::{perlin_noise_float_2d, perlin_noise_float_3d, perlin_noise_vec3_2d, perlin_noise_vec3_3d};

// mx_fractal2d_noise_float
pub fn fractal_noise_float_2d(mut p: [f32; 2], octaves: i32, lacunarity: f32, diminish: f32) -> f32 {
    let mut result = 0.0;
    let mut amplitude = 1.0;
    for _ in 0..octaves {
        result += amplitude * perlin_noise_float_2d(p);
        amplitude *= diminish;
        p = [p[0] * lacunarity, p[1] * lacunarity];
    }
    result
}

// mx_fractal2d_noise_vec3
pub fn fractal_noise_vec3_2d(mut p: [f32; 2], octaves: i32, lacunarity: f32, diminish: f32) -> [f32; 3] {
    let mut result = [0.0; 3];
    let mut amplitude = 1.0;
    for _ in 0..octaves {
        let n = perlin_noise_vec3_2d(p);
        result = [result[0] + amplitude * n[0], result[1] + amplitude * n[1], result[2] + amplitude * n[2]];
        amplitude *= diminish;
        p = [p[0] * lacunarity, p[1] * lacunarity];
    }
    result
}

// mx_fractal2d_noise_vec2
pub fn fractal_noise_vec2_2d(p: [f32; 2], octaves: i32, lacunarity: f32, diminish: f32) -> [f32; 2] {
    [
        fractal_noise_float_2d(p, octaves, lacunarity, diminish),
        fractal_noise_float_2d([p[0] + 19.0, p[1] + 193.0], octaves, lacunarity, diminish),
    ]
}

// mx_fractal2d_noise_vec4
pub fn fractal_noise_vec4_2d(p: [f32; 2], octaves: i32, lacunarity: f32, diminish: f32) -> [f32; 4] {
    let c = fractal_noise_vec3_2d(p, octaves, lacunarity, diminish);
    let f = fractal_noise_float_2d([p[0] + 19.0, p[1] + 193.0], octaves, lacunarity, diminish);
    [c[0], c[1], c[2], f]
}

// mx_fractal3d_noise_float
pub fn fractal_noise_float_3d(mut p: [f32; 3], octaves: i32, lacunarity: f32, diminish: f32) -> f32 {
    let mut result = 0.0;
    let mut amplitude = 1.0;
    for _ in 0..octaves {
        result += amplitude * perlin_noise_float_3d(p);
        amplitude *= diminish;
        p = [p[0] * lacunarity, p[1] * lacunarity, p[2] * lacunarity];
    }
    result
}

// mx_fractal3d_noise_vec3
pub fn fractal_noise_vec3_3d(mut p: [f32; 3], octaves: i32, lacunarity: f32, diminish: f32) -> [f32; 3] {
    let mut result = [0.0; 3];
    let mut amplitude = 1.0;
    for _ in 0..octaves {
        let n = perlin_noise_vec3_3d(p);
        result = [result[0] + amplitude * n[0], result[1] + amplitude * n[1], result[2] + amplitude * n[2]];
        amplitude *= diminish;
        p = [p[0] * lacunarity, p[1] * lacunarity, p[2] * lacunarity];
    }
    result
}

// mx_fractal3d_noise_vec2
pub fn fractal_noise_vec2_3d(p: [f32; 3], octaves: i32, lacunarity: f32, diminish: f32) -> [f32; 2] {
    [
        fractal_noise_float_3d(p, octaves, lacunarity, diminish),
        fractal_noise_float_3d([p[0] + 19.0, p[1] + 193.0, p[2] + 17.0], octaves, lacunarity, diminish),
    ]
}

// mx_fractal3d_noise_vec4
pub fn fractal_noise_vec4_3d(p: [f32; 3], octaves: i32, lacunarity: f32, diminish: f32) -> [f32; 4] {
    let c = fractal_noise_vec3_3d(p, octaves, lacunarity, diminish);
    let f = fractal_noise_float_3d([p[0] + 19.0, p[1] + 193.0, p[2] + 17.0], octaves, lacunarity, diminish);
    [c[0], c[1], c[2], f]
}
