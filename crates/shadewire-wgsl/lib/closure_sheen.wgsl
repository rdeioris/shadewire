// Sheen: ported from MaterialX pbrlib genglsl (lib/mx_microfacet_sheen.glsl and the sheen BSDF node), with the
// analytic directional albedo. Needs closure_base.

// https://fpsunflower.github.io/ckulla/data/s2017_pbs_imageworks_sheen.pdf Equation 2
fn shadewire_mx_imageworks_sheen_ndf(n_dot_h: f32, roughness: f32) -> f32 {
    let inv_roughness = 1.0 / max(roughness, 0.005);
    let cos2 = n_dot_h * n_dot_h;
    let sin2 = 1.0 - cos2;
    return (2.0 + inv_roughness) * pow(sin2, inv_roughness * 0.5) / (2.0 * SHADEWIRE_PI);
}

fn shadewire_mx_imageworks_sheen_brdf(n_dot_l: f32, n_dot_v: f32, n_dot_h: f32, roughness: f32) -> f32 {
    // Fresnel and geometry terms are ignored; a smoother denominator, as in
    // https://blog.selfshadow.com/publications/s2013-shading-course/rad/s2013_pbs_rad_notes.pdf
    let d = shadewire_mx_imageworks_sheen_ndf(n_dot_h, roughness);
    return d / (4.0 * (n_dot_l + n_dot_v - n_dot_l * n_dot_v));
}

// Rational quadratic fit to Monte Carlo data for Imageworks sheen directional albedo.
fn shadewire_mx_imageworks_sheen_dir_albedo(n_dot_v: f32, roughness: f32) -> f32 {
    let r = vec2<f32>(13.67300, 1.0) +
            vec2<f32>(-68.78018, 61.57746) * n_dot_v +
            vec2<f32>(799.08825, 442.78211) * roughness +
            vec2<f32>(-905.00061, 2597.49308) * n_dot_v * roughness +
            vec2<f32>(60.28956, 121.81241) * n_dot_v * n_dot_v +
            vec2<f32>(1086.96473, 3045.55075) * roughness * roughness;
    return clamp(r.x / r.y, 0.0, 1.0);
}

// "Practical Multiple-Scattering Sheen Using Linearly Transformed Cosines", Zeltner et al.
// https://github.com/tizian/ltc-sheen. Gaussian fit to the directional albedo table.
fn shadewire_mx_zeltner_sheen_dir_albedo(x: f32, y: f32) -> f32 {
    let s = y * (0.0206607 + 1.58491 * y) / (0.0379424 + y * (1.32227 + y));
    let m = y * (-0.193854 + y * (-1.14885 + y * (1.7932 - 0.95943 * y * y))) / (0.046391 + y);
    let o = y * (0.000654023 + (-0.0207818 + 0.119681 * y) * y) / (1.26264 + y * (-1.92021 + y));
    let z = (x - m) / s;
    return exp(-0.5 * z * z) / (s * sqrt(2.0 * SHADEWIRE_PI)) + o;
}

// Rational fits to the LTC matrix coefficients.
fn shadewire_mx_zeltner_sheen_ltc_a_inv(x: f32, y: f32) -> f32 {
    return (2.58126 * x + 0.813703 * y) * y / (1.0 + 0.310327 * x * x + 2.60994 * x * y);
}

fn shadewire_mx_zeltner_sheen_ltc_b_inv(x: f32, y: f32) -> f32 {
    return sqrt(1.0 - x) * (y - 1.0) * y * y * y / (0.0000254053 + 1.71228 * x - 1.71506 * x * y + 1.34174 * y * y);
}

// A basis with its tangent in the plane of V and N, which orients the LTC lobe.
fn shadewire_mx_orthonormal_basis_ltc(v: vec3<f32>, n: vec3<f32>, n_dot_v: f32) -> mat3x3<f32> {
    var x = v - n * n_dot_v;
    let len_sqr = dot(x, x);
    if (len_sqr > 0.0) {
        x *= inverseSqrt(len_sqr);
        return mat3x3<f32>(x, cross(n, x), n);
    }
    // V == N: any basis will do.
    return shadewire_mx_orthonormal_basis(n);
}

// Multiplication by the directional albedo is the caller's.
fn shadewire_mx_zeltner_sheen_brdf(l: vec3<f32>, v: vec3<f32>, n: vec3<f32>, n_dot_v: f32, roughness: f32) -> f32 {
    let to_ltc = transpose(shadewire_mx_orthonormal_basis_ltc(v, n, n_dot_v));
    let w = to_ltc * l;
    let a_inv = shadewire_mx_zeltner_sheen_ltc_a_inv(n_dot_v, roughness);
    let b_inv = shadewire_mx_zeltner_sheen_ltc_b_inv(n_dot_v, roughness);
    // w back to the original configuration (clamped cosine).
    let wo = vec3<f32>(a_inv * w.x + b_inv * w.z, a_inv * w.y, w.z);
    let len_sqr = dot(wo, wo);
    let k = a_inv / len_sqr;
    return shadewire_mx_cosine_hemisphere_pdf(wo.z) * k * k;
}

// ------------------------------------------------------------------------------------------------- lobe

// `mode`: 0 Conty-Kulla (Imageworks), 1 Zeltner.
fn shadewire_sheen_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, color: vec3<f32>, roughness: f32, normal: vec3<f32>, mode: i32) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return shadewire_bsdf_none();
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    var dir_albedo: f32;
    var response: vec3<f32>;
    if (mode == 0) {
        let h = normalize(l + v);
        let n_dot_l = clamp(dot(n, l), SHADEWIRE_EPS, 1.0);
        let n_dot_h = clamp(dot(n, h), SHADEWIRE_EPS, 1.0);
        let fr = color * shadewire_mx_imageworks_sheen_brdf(n_dot_l, n_dot_v, n_dot_h, roughness);
        dir_albedo = shadewire_mx_imageworks_sheen_dir_albedo(n_dot_v, roughness);
        // NdotL from the light integral: the BRDF's denominator does not cancel it.
        response = fr * n_dot_l * weight;
    } else {
        let r = clamp(roughness, 0.01, 1.0);  // the range of the original implementation
        let fr = color * shadewire_mx_zeltner_sheen_brdf(l, v, n, n_dot_v, r);
        dir_albedo = shadewire_mx_zeltner_sheen_dir_albedo(n_dot_v, r);
        response = dir_albedo * fr * weight;
    }
    return ShadewireBsdf(response, vec3<f32>(1.0 - dir_albedo * weight));
}

fn shadewire_sheen_bsdf_albedo(v: vec3<f32>, weight: f32, color: vec3<f32>, roughness: f32, normal: vec3<f32>, mode: i32) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return shadewire_bsdf_none();
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    var dir_albedo: f32;
    if (mode == 0) {
        dir_albedo = shadewire_mx_imageworks_sheen_dir_albedo(n_dot_v, roughness);
    } else {
        dir_albedo = shadewire_mx_zeltner_sheen_dir_albedo(n_dot_v, clamp(roughness, 0.01, 1.0));
    }
    return ShadewireBsdf(color * dir_albedo * weight, vec3<f32>(1.0 - dir_albedo * weight));
}
