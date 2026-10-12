// Port of MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0, see NOTICE).
// Derived from Open Shading Language oslnoise.h, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc.,
// et al., BSD-3-Clause (see NOTICE). Mirrors crates/shadewire/src/ops/noise/fractal.rs.
// Needs noise_perlin.

// mx_fractal2d_noise_float (Rust fractal_noise_float_2d)
float shadewire_fractal_noise_float_2d(vec2 p_in, int oct, float lac, float dim) {
    vec2 p = p_in;
    float result = 0.0;
    float amplitude = 1.0;
    for (int i = 0; i < oct; i++) {
        result += amplitude * shadewire_perlin_noise_float_2d(p);
        amplitude *= dim;
        p *= lac;
    }
    return result;
}

// mx_fractal2d_noise_vec3 (Rust fractal_noise_vec3_2d)
vec3 shadewire_fractal_noise_vec3_2d(vec2 p_in, int oct, float lac, float dim) {
    vec2 p = p_in;
    vec3 result = vec3(0.0);
    float amplitude = 1.0;
    for (int i = 0; i < oct; i++) {
        result += amplitude * shadewire_perlin_noise_vec3_2d(p);
        amplitude *= dim;
        p *= lac;
    }
    return result;
}

// mx_fractal2d_noise_vec2 (Rust fractal_noise_vec2_2d)
vec2 shadewire_fractal_noise_vec2_2d(vec2 p, int oct, float lac, float dim) {
    return vec2(
        shadewire_fractal_noise_float_2d(p, oct, lac, dim),
        shadewire_fractal_noise_float_2d(p + vec2(19.0, 193.0), oct, lac, dim));
}

// mx_fractal2d_noise_vec4 (Rust fractal_noise_vec4_2d)
vec4 shadewire_fractal_noise_vec4_2d(vec2 p, int oct, float lac, float dim) {
    vec3 c = shadewire_fractal_noise_vec3_2d(p, oct, lac, dim);
    float f = shadewire_fractal_noise_float_2d(p + vec2(19.0, 193.0), oct, lac, dim);
    return vec4(c, f);
}

// mx_fractal3d_noise_float (Rust fractal_noise_float_3d)
float shadewire_fractal_noise_float_3d(vec3 p_in, int oct, float lac, float dim) {
    vec3 p = p_in;
    float result = 0.0;
    float amplitude = 1.0;
    for (int i = 0; i < oct; i++) {
        result += amplitude * shadewire_perlin_noise_float_3d(p);
        amplitude *= dim;
        p *= lac;
    }
    return result;
}

// mx_fractal3d_noise_vec3 (Rust fractal_noise_vec3_3d)
vec3 shadewire_fractal_noise_vec3_3d(vec3 p_in, int oct, float lac, float dim) {
    vec3 p = p_in;
    vec3 result = vec3(0.0);
    float amplitude = 1.0;
    for (int i = 0; i < oct; i++) {
        result += amplitude * shadewire_perlin_noise_vec3_3d(p);
        amplitude *= dim;
        p *= lac;
    }
    return result;
}

// mx_fractal3d_noise_vec2 (Rust fractal_noise_vec2_3d)
vec2 shadewire_fractal_noise_vec2_3d(vec3 p, int oct, float lac, float dim) {
    return vec2(
        shadewire_fractal_noise_float_3d(p, oct, lac, dim),
        shadewire_fractal_noise_float_3d(p + vec3(19.0, 193.0, 17.0), oct, lac, dim));
}

// mx_fractal3d_noise_vec4 (Rust fractal_noise_vec4_3d)
vec4 shadewire_fractal_noise_vec4_3d(vec3 p, int oct, float lac, float dim) {
    vec3 c = shadewire_fractal_noise_vec3_3d(p, oct, lac, dim);
    float f = shadewire_fractal_noise_float_3d(p + vec3(19.0, 193.0, 17.0), oct, lac, dim);
    return vec4(c, f);
}
