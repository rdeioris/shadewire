// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire-ir/src/ops/noise/fractal.rs.
// Needs noise_perlin.

// mx_fractal2d_noise_float (Rust fractal_noise_float_2d)
fn shadewire_fractal_noise_float_2d(p_in: vec2<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> f32 {
    var p = p_in;
    var result = 0.0;
    var amplitude = 1.0;
    for (var i = 0; i < octaves; i++) {
        result += amplitude * shadewire_perlin_noise_float_2d(p);
        amplitude *= diminish;
        p *= lacunarity;
    }
    return result;
}

// mx_fractal2d_noise_vec3 (Rust fractal_noise_vec3_2d)
fn shadewire_fractal_noise_vec3_2d(p_in: vec2<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> vec3<f32> {
    var p = p_in;
    var result = vec3<f32>(0.0);
    var amplitude = 1.0;
    for (var i = 0; i < octaves; i++) {
        result += amplitude * shadewire_perlin_noise_vec3_2d(p);
        amplitude *= diminish;
        p *= lacunarity;
    }
    return result;
}

// mx_fractal2d_noise_vec2 (Rust fractal_noise_vec2_2d)
fn shadewire_fractal_noise_vec2_2d(p: vec2<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> vec2<f32> {
    return vec2<f32>(
        shadewire_fractal_noise_float_2d(p, octaves, lacunarity, diminish),
        shadewire_fractal_noise_float_2d(p + vec2<f32>(19.0, 193.0), octaves, lacunarity, diminish),
    );
}

// mx_fractal2d_noise_vec4 (Rust fractal_noise_vec4_2d)
fn shadewire_fractal_noise_vec4_2d(p: vec2<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> vec4<f32> {
    let c = shadewire_fractal_noise_vec3_2d(p, octaves, lacunarity, diminish);
    let f = shadewire_fractal_noise_float_2d(p + vec2<f32>(19.0, 193.0), octaves, lacunarity, diminish);
    return vec4<f32>(c, f);
}

// mx_fractal3d_noise_float (Rust fractal_noise_float_3d)
fn shadewire_fractal_noise_float_3d(p_in: vec3<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> f32 {
    var p = p_in;
    var result = 0.0;
    var amplitude = 1.0;
    for (var i = 0; i < octaves; i++) {
        result += amplitude * shadewire_perlin_noise_float_3d(p);
        amplitude *= diminish;
        p *= lacunarity;
    }
    return result;
}

// mx_fractal3d_noise_vec3 (Rust fractal_noise_vec3_3d)
fn shadewire_fractal_noise_vec3_3d(p_in: vec3<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> vec3<f32> {
    var p = p_in;
    var result = vec3<f32>(0.0);
    var amplitude = 1.0;
    for (var i = 0; i < octaves; i++) {
        result += amplitude * shadewire_perlin_noise_vec3_3d(p);
        amplitude *= diminish;
        p *= lacunarity;
    }
    return result;
}

// mx_fractal3d_noise_vec2 (Rust fractal_noise_vec2_3d)
fn shadewire_fractal_noise_vec2_3d(p: vec3<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> vec2<f32> {
    return vec2<f32>(
        shadewire_fractal_noise_float_3d(p, octaves, lacunarity, diminish),
        shadewire_fractal_noise_float_3d(p + vec3<f32>(19.0, 193.0, 17.0), octaves, lacunarity, diminish),
    );
}

// mx_fractal3d_noise_vec4 (Rust fractal_noise_vec4_3d)
fn shadewire_fractal_noise_vec4_3d(p: vec3<f32>, octaves: i32, lacunarity: f32, diminish: f32) -> vec4<f32> {
    let c = shadewire_fractal_noise_vec3_3d(p, octaves, lacunarity, diminish);
    let f = shadewire_fractal_noise_float_3d(p + vec3<f32>(19.0, 193.0, 17.0), octaves, lacunarity, diminish);
    return vec4<f32>(c, f);
}
