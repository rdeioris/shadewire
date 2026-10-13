// Diffuse lobes: ported from MaterialX pbrlib genglsl (lib/mx_microfacet_diffuse.glsl and the oren_nayar, burley,
// translucent and subsurface BSDF nodes). Needs closure_base.

const SHADEWIRE_FUJII_CONSTANT_1: f32 = 0.5 - 2.0 / (3.0 * SHADEWIRE_PI);
const SHADEWIRE_FUJII_CONSTANT_2: f32 = 2.0 / 3.0 - 28.0 / (15.0 * SHADEWIRE_PI);

// Qualitative Oren-Nayar diffuse with simplified math:
// https://www1.cs.columbia.edu/CAVE/publications/pdfs/Oren_SIGGRAPH94.pdf
fn shadewire_mx_oren_nayar_diffuse(n_dot_v: f32, n_dot_l: f32, l_dot_v: f32, roughness: f32) -> f32 {
    let s = l_dot_v - n_dot_l * n_dot_v;
    let stinv = select(0.0, s / max(n_dot_l, n_dot_v), s > 0.0);
    let sigma2 = roughness * roughness;
    let a = 1.0 - 0.5 * (sigma2 / (sigma2 + 0.33));
    let b = 0.45 * sigma2 / (sigma2 + 0.09);
    return a + b * stinv;
}

// Rational quadratic fit to Monte Carlo data for Oren-Nayar directional albedo.
fn shadewire_mx_oren_nayar_diffuse_dir_albedo(n_dot_v: f32, roughness: f32) -> f32 {
    let r = vec2<f32>(1.0, 1.0) +
            vec2<f32>(-0.4297, -0.6076) * roughness +
            vec2<f32>(-0.7632, -0.4993) * n_dot_v * roughness +
            vec2<f32>(1.4385, 2.0315) * roughness * roughness;
    return clamp(r.x / r.y, 0.0, 1.0);
}

// Improved Oren-Nayar diffuse from Fujii: https://mimosa-pudica.net/improved-oren-nayar.html
fn shadewire_mx_oren_nayar_fujii_diffuse_dir_albedo(cos_theta: f32, roughness: f32) -> f32 {
    let a = 1.0 / (1.0 + SHADEWIRE_FUJII_CONSTANT_1 * roughness);
    let b = roughness * a;
    let si = sqrt(max(0.0, 1.0 - cos_theta * cos_theta));
    let g = si * (acos(clamp(cos_theta, -1.0, 1.0)) - si * cos_theta) +
            2.0 * ((si / cos_theta) * (1.0 - si * si * si) - si) / 3.0;
    return a + (b * g * SHADEWIRE_PI_INV);
}

fn shadewire_mx_oren_nayar_fujii_diffuse_avg_albedo(roughness: f32) -> f32 {
    let a = 1.0 / (1.0 + SHADEWIRE_FUJII_CONSTANT_1 * roughness);
    return a * (1.0 + SHADEWIRE_FUJII_CONSTANT_2 * roughness);
}

// Energy-compensated Oren-Nayar diffuse from OpenPBR Surface: https://academysoftwarefoundation.github.io/OpenPBR/
fn shadewire_mx_oren_nayar_compensated_diffuse(n_dot_v: f32, n_dot_l: f32, l_dot_v: f32, roughness: f32, color: vec3<f32>) -> vec3<f32> {
    let s = l_dot_v - n_dot_l * n_dot_v;
    let stinv = select(s, s / max(n_dot_l, n_dot_v), s > 0.0);

    // The single-scatter lobe.
    let a = 1.0 / (1.0 + SHADEWIRE_FUJII_CONSTANT_1 * roughness);
    let lobe_single_scatter = color * a * (1.0 + roughness * stinv);

    // The multi-scatter lobe.
    let dir_albedo_v = shadewire_mx_oren_nayar_fujii_diffuse_dir_albedo(n_dot_v, roughness);
    let dir_albedo_l = shadewire_mx_oren_nayar_fujii_diffuse_dir_albedo(n_dot_l, roughness);
    let avg_albedo = shadewire_mx_oren_nayar_fujii_diffuse_avg_albedo(roughness);
    let color_multi_scatter = color * color * avg_albedo / (vec3<f32>(1.0) - color * max(0.0, 1.0 - avg_albedo));
    let lobe_multi_scatter = color_multi_scatter *
                             max(SHADEWIRE_EPS, 1.0 - dir_albedo_v) *
                             max(SHADEWIRE_EPS, 1.0 - dir_albedo_l) /
                             max(SHADEWIRE_EPS, 1.0 - avg_albedo);
    return lobe_single_scatter + lobe_multi_scatter;
}

fn shadewire_mx_oren_nayar_compensated_diffuse_dir_albedo(cos_theta: f32, roughness: f32, color: vec3<f32>) -> vec3<f32> {
    let dir_albedo = shadewire_mx_oren_nayar_fujii_diffuse_dir_albedo(cos_theta, roughness);
    let avg_albedo = shadewire_mx_oren_nayar_fujii_diffuse_avg_albedo(roughness);
    let color_multi_scatter = color * color * avg_albedo / (vec3<f32>(1.0) - color * max(0.0, 1.0 - avg_albedo));
    return mix(color_multi_scatter, color, vec3<f32>(dir_albedo));
}

// https://media.disneyanimation.com/uploads/production/publication_asset/48/asset/s2012_pbs_disney_brdf_notes_v3.pdf
// Section 5.3
fn shadewire_mx_burley_diffuse(n_dot_v: f32, n_dot_l: f32, l_dot_h: f32, roughness: f32) -> f32 {
    let f90 = 0.5 + (2.0 * roughness * l_dot_h * l_dot_h);
    let ref_l = shadewire_mx_fresnel_schlick_float(n_dot_l, 1.0, f90);
    let ref_v = shadewire_mx_fresnel_schlick_float(n_dot_v, 1.0, f90);
    return ref_l * ref_v;
}

// The directional albedo of Burley diffuse. Curve fit provided by Stephen Hill.
fn shadewire_mx_burley_diffuse_dir_albedo(n_dot_v: f32, roughness: f32) -> f32 {
    let x = n_dot_v;
    let fit0 = 0.97619 - 0.488095 * shadewire_mx_pow5(1.0 - x);
    let fit1 = 1.55754 + (-2.02221 + (2.56283 - 1.06244 * x) * x) * x;
    return mix(fit0, fit1, roughness);
}

// ------------------------------------------------------------------------------------------------- lobes

fn shadewire_oren_nayar_diffuse_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, color: vec3<f32>, roughness: f32, normal: vec3<f32>, energy_compensation: bool) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    let n_dot_l = clamp(dot(n, l), SHADEWIRE_EPS, 1.0);
    let l_dot_v = clamp(dot(l, v), SHADEWIRE_EPS, 1.0);
    var diffuse: vec3<f32>;
    if (energy_compensation) {
        diffuse = shadewire_mx_oren_nayar_compensated_diffuse(n_dot_v, n_dot_l, l_dot_v, roughness, color);
    } else {
        diffuse = shadewire_mx_oren_nayar_diffuse(n_dot_v, n_dot_l, l_dot_v, roughness) * color;
    }
    return ShadewireBsdf(diffuse * weight * n_dot_l * SHADEWIRE_PI_INV, vec3<f32>(0.0));
}

fn shadewire_oren_nayar_diffuse_bsdf_albedo(v: vec3<f32>, weight: f32, color: vec3<f32>, roughness: f32, normal: vec3<f32>, energy_compensation: bool) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    var diffuse: vec3<f32>;
    if (energy_compensation) {
        diffuse = shadewire_mx_oren_nayar_compensated_diffuse_dir_albedo(n_dot_v, roughness, color);
    } else {
        diffuse = shadewire_mx_oren_nayar_diffuse_dir_albedo(n_dot_v, roughness) * color;
    }
    return ShadewireBsdf(diffuse * weight, vec3<f32>(0.0));
}

fn shadewire_burley_diffuse_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, color: vec3<f32>, roughness: f32, normal: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    let n_dot_l = clamp(dot(n, l), SHADEWIRE_EPS, 1.0);
    let l_dot_h = clamp(dot(l, normalize(l + v)), SHADEWIRE_EPS, 1.0);
    let response = color * weight * n_dot_l * SHADEWIRE_PI_INV * shadewire_mx_burley_diffuse(n_dot_v, n_dot_l, l_dot_h, roughness);
    return ShadewireBsdf(response, vec3<f32>(0.0));
}

fn shadewire_burley_diffuse_bsdf_albedo(v: vec3<f32>, weight: f32, color: vec3<f32>, roughness: f32, normal: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    return ShadewireBsdf(color * weight * shadewire_mx_burley_diffuse_dir_albedo(n_dot_v, roughness), vec3<f32>(0.0));
}

// Light arriving from behind the surface, diffused through it.
fn shadewire_translucent_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, color: vec3<f32>, normal: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let n_dot_l = clamp(dot(-normal, l), 0.0, 1.0);
    return ShadewireBsdf(color * weight * n_dot_l * SHADEWIRE_PI_INV, vec3<f32>(0.0));
}

fn shadewire_translucent_bsdf_albedo(v: vec3<f32>, weight: f32, color: vec3<f32>, normal: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    return ShadewireBsdf(color * weight, vec3<f32>(0.0));
}

// MaterialX's GLSL estimates the diffusion radius from screen-space curvature, which needs the shading position
// and fragment-stage derivatives; here subsurface scatters as Lambertian diffuse of its color, as MaterialX's
// GLSL renders indirect subsurface. `radius` and `anisotropy` are not read.
fn shadewire_subsurface_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, color: vec3<f32>, radius: vec3<f32>, anisotropy: f32, normal: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let n = shadewire_mx_forward_facing_normal(normal, v);
    let n_dot_l = clamp(dot(n, l), SHADEWIRE_EPS, 1.0);
    return ShadewireBsdf(color * weight * n_dot_l * SHADEWIRE_PI_INV, vec3<f32>(0.0));
}

fn shadewire_subsurface_bsdf_albedo(v: vec3<f32>, weight: f32, color: vec3<f32>, radius: vec3<f32>, anisotropy: f32, normal: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    return ShadewireBsdf(color * weight, vec3<f32>(0.0));
}
