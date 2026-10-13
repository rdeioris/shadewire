// Microfacet specular lobes: ported from MaterialX pbrlib genglsl (lib/mx_microfacet_specular.glsl and the
// dielectric, conductor and generalized_schlick BSDF nodes), with the analytic directional albedo. Light
// arriving from behind the surface is not transmitted (MaterialX's GLSL refracts only its environment).
// Needs closure_base.

const SHADEWIRE_FRESNEL_DIELECTRIC: i32 = 0;
const SHADEWIRE_FRESNEL_CONDUCTOR: i32 = 1;
const SHADEWIRE_FRESNEL_SCHLICK: i32 = 2;
const SHADEWIRE_AIRY_FRESNEL_ITERATIONS: i32 = 2;

// The parameters of a Fresnel term: physical (dielectric, conductor) or generalized Schlick, with an optional
// thin film (`airy`).
struct ShadewireFresnel {
    model: i32,
    airy: bool,
    ior: vec3<f32>,
    extinction: vec3<f32>,
    f0: vec3<f32>,
    f82: vec3<f32>,
    f90: vec3<f32>,
    exponent: f32,
    tf_thickness: f32,
    tf_ior: f32,
}

// Parallel and perpendicular parts of a polarized quantity.
struct ShadewirePolarized {
    p: vec3<f32>,
    s: vec3<f32>,
}

// https://media.disneyanimation.com/uploads/production/publication_asset/48/asset/s2012_pbs_disney_brdf_notes_v3.pdf
// Appendix B.2 Equation 13
fn shadewire_mx_ggx_ndf(h: vec3<f32>, alpha: vec2<f32>) -> f32 {
    let he = h.xy / alpha;
    let denom = dot(he, he) + h.z * h.z;
    return 1.0 / (SHADEWIRE_PI * alpha.x * alpha.y * denom * denom);
}

// Height-correlated Smith masking-shadowing. http://jcgt.org/published/0003/02/03/paper.pdf Equations 72 and 99
fn shadewire_mx_ggx_smith_g2(n_dot_l: f32, n_dot_v: f32, alpha: f32) -> f32 {
    let alpha2 = alpha * alpha;
    let lambda_l = sqrt(alpha2 + (1.0 - alpha2) * n_dot_l * n_dot_l);
    let lambda_v = sqrt(alpha2 + (1.0 - alpha2) * n_dot_v * n_dot_v);
    return 2.0 * n_dot_l * n_dot_v / (lambda_l * n_dot_v + lambda_v * n_dot_l);
}

// Rational quadratic fit to Monte Carlo data for GGX directional albedo.
fn shadewire_mx_ggx_dir_albedo(n_dot_v: f32, alpha: f32, f0: vec3<f32>, f90: vec3<f32>) -> vec3<f32> {
    let x = n_dot_v;
    let y = alpha;
    let x2 = x * x;
    let y2 = y * y;
    let r = vec4<f32>(0.1003, 0.9345, 1.0, 1.0) +
            vec4<f32>(-0.6303, -2.323, -1.765, 0.2281) * x +
            vec4<f32>(9.748, 2.229, 8.263, 15.94) * y +
            vec4<f32>(-2.038, -3.748, 11.53, -55.83) * x * y +
            vec4<f32>(29.34, 1.424, 28.96, 13.08) * x2 +
            vec4<f32>(-8.245, -0.7684, -7.507, 41.26) * y2 +
            vec4<f32>(-26.44, 1.436, -36.11, 54.9) * x2 * y +
            vec4<f32>(19.99, 0.2913, 15.86, 300.2) * x * y2 +
            vec4<f32>(-5.448, 0.6286, 33.37, -285.1) * x2 * y2;
    let ab = clamp(r.xy / r.zw, vec2<f32>(0.0), vec2<f32>(1.0));
    return f0 * ab.x + f90 * ab.y;
}

// https://blog.selfshadow.com/publications/turquin/ms_comp_final.pdf Equations 14 and 16
fn shadewire_mx_ggx_energy_compensation(n_dot_v: f32, alpha: f32, fss: vec3<f32>) -> vec3<f32> {
    let ess = shadewire_mx_ggx_dir_albedo(n_dot_v, alpha, vec3<f32>(1.0), vec3<f32>(1.0)).x;
    return 1.0 + fss * (1.0 - ess) / ess;
}

fn shadewire_mx_average_alpha(alpha: vec2<f32>) -> f32 {
    return sqrt(alpha.x * alpha.y);
}

fn shadewire_mx_ior_to_f0(ior: f32) -> f32 {
    let r = (ior - 1.0) / (ior + 1.0);
    return r * r;
}

fn shadewire_mx_f0_to_ior(f0: vec3<f32>) -> vec3<f32> {
    let sqrt_f0 = sqrt(clamp(f0, vec3<f32>(0.01), vec3<f32>(0.99)));
    return (vec3<f32>(1.0) + sqrt_f0) / (vec3<f32>(1.0) - sqrt_f0);
}

// https://renderwonk.com/publications/wp-generalization-adobe/gen-adobe.pdf
fn shadewire_mx_fresnel_hoffman_schlick(cos_theta: f32, fd: ShadewireFresnel) -> vec3<f32> {
    let cos_theta_max = 1.0 / 7.0;
    let cos_theta_factor = 1.0 / (cos_theta_max * pow(1.0 - cos_theta_max, 6.0));
    let x = clamp(cos_theta, 0.0, 1.0);
    let a = mix(fd.f0, fd.f90, vec3<f32>(pow(1.0 - cos_theta_max, fd.exponent))) * (vec3<f32>(1.0) - fd.f82) * cos_theta_factor;
    return mix(fd.f0, fd.f90, vec3<f32>(pow(1.0 - x, fd.exponent))) - a * x * shadewire_mx_pow6(1.0 - x);
}

// https://seblagarde.wordpress.com/2013/04/29/memo-on-fresnel-equations/
fn shadewire_mx_fresnel_dielectric(cos_theta: f32, ior: f32) -> f32 {
    let c = cos_theta;
    let g2 = ior * ior + c * c - 1.0;
    if (g2 < 0.0) {
        return 1.0;  // total internal reflection
    }
    let g = sqrt(g2);
    let a = (g - c) / (g + c);
    let b = ((g + c) * c - 1.0) / ((g - c) * c + 1.0);
    return 0.5 * a * a * (1.0 + b * b);
}

// https://seblagarde.wordpress.com/2013/04/29/memo-on-fresnel-equations/
fn shadewire_mx_fresnel_dielectric_polarized(cos_theta: f32, ior: f32) -> vec2<f32> {
    let cos_theta2 = clamp(cos_theta, 0.0, 1.0) * clamp(cos_theta, 0.0, 1.0);
    let sin_theta2 = 1.0 - cos_theta2;
    let t0 = max(ior * ior - sin_theta2, 0.0);
    let t1 = t0 + cos_theta2;
    let t2 = 2.0 * sqrt(t0) * cos_theta;
    let rs = (t1 - t2) / (t1 + t2);
    let t3 = cos_theta2 * t0 + sin_theta2 * sin_theta2;
    let t4 = t2 * sin_theta2;
    let rp = rs * (t3 - t4) / (t3 + t4);
    return vec2<f32>(rp, rs);
}

// https://seblagarde.wordpress.com/2013/04/29/memo-on-fresnel-equations/
fn shadewire_mx_fresnel_conductor_polarized(cos_theta: f32, n: vec3<f32>, k: vec3<f32>) -> ShadewirePolarized {
    let cos_theta2 = clamp(cos_theta, 0.0, 1.0) * clamp(cos_theta, 0.0, 1.0);
    let sin_theta2 = 1.0 - cos_theta2;
    let n2 = n * n;
    let k2 = k * k;
    let t0 = n2 - k2 - vec3<f32>(sin_theta2);
    let a2plusb2 = sqrt(t0 * t0 + 4.0 * n2 * k2);
    let t1 = a2plusb2 + vec3<f32>(cos_theta2);
    let a = sqrt(max(0.5 * (a2plusb2 + t0), vec3<f32>(0.0)));
    let t2 = 2.0 * a * cos_theta;
    let rs = (t1 - t2) / (t1 + t2);
    let t3 = cos_theta2 * a2plusb2 + vec3<f32>(sin_theta2 * sin_theta2);
    let t4 = t2 * sin_theta2;
    let rp = rs * (t3 - t4) / (t3 + t4);
    return ShadewirePolarized(rp, rs);
}

fn shadewire_mx_fresnel_conductor(cos_theta: f32, n: vec3<f32>, k: vec3<f32>) -> vec3<f32> {
    let r = shadewire_mx_fresnel_conductor_polarized(cos_theta, n, k);
    return 0.5 * (r.p + r.s);
}

// https://belcour.github.io/blog/research/publication/2017/05/01/brdf-thin-film.html
fn shadewire_mx_fresnel_conductor_phase_polarized(cos_theta: f32, eta1: f32, eta2: vec3<f32>, kappa2: vec3<f32>) -> ShadewirePolarized {
    let k2 = kappa2 / eta2;
    let sin_theta_sqr = vec3<f32>(1.0) - cos_theta * cos_theta;
    let a = eta2 * eta2 * (vec3<f32>(1.0) - k2 * k2) - eta1 * eta1 * sin_theta_sqr;
    let b = sqrt(a * a + (2.0 * eta2 * eta2 * k2) * (2.0 * eta2 * eta2 * k2));
    let u = sqrt((a + b) / 2.0);
    let v = max(vec3<f32>(0.0), sqrt((b - a) / 2.0));
    let phi_s = atan2(2.0 * eta1 * v * cos_theta, u * u + v * v - (eta1 * cos_theta) * (eta1 * cos_theta));
    let c = eta2 * eta2 * (vec3<f32>(1.0) + k2 * k2) * cos_theta;
    let phi_p = atan2(2.0 * eta1 * eta2 * eta2 * cos_theta * (2.0 * k2 * u - (vec3<f32>(1.0) - k2 * k2) * v),
                      c * c - eta1 * eta1 * (u * u + v * v));
    return ShadewirePolarized(phi_p, phi_s);
}

// https://belcour.github.io/blog/research/publication/2017/05/01/brdf-thin-film.html
fn shadewire_mx_eval_sensitivity(opd: f32, shift: vec3<f32>) -> vec3<f32> {
    // Gaussian fits, given by 3 parameters: val, pos and var.
    let phase = 2.0 * SHADEWIRE_PI * opd;
    let val = vec3<f32>(5.4856e-13, 4.4201e-13, 5.2481e-13);
    let pos = vec3<f32>(1.6810e+06, 1.7953e+06, 2.2084e+06);
    let variance = vec3<f32>(4.3278e+09, 9.3046e+09, 6.6121e+09);
    var xyz = val * sqrt(2.0 * SHADEWIRE_PI * variance) * cos(pos * phase + shift) * exp(-variance * phase * phase);
    xyz.x += 9.7470e-14 * sqrt(2.0 * SHADEWIRE_PI * 4.5282e+09) * cos(2.2399e+06 * phase + shift.x) * exp(-4.5282e+09 * phase * phase);
    return xyz / 1.0685e-7;
}

// A Practical Extension to Microfacet Theory for the Modeling of Varying Iridescence
// https://belcour.github.io/blog/research/publication/2017/05/01/brdf-thin-film.html
fn shadewire_mx_fresnel_airy(cos_theta: f32, fd: ShadewireFresnel) -> vec3<f32> {
    // XYZ to CIE 1931 RGB color space (using neutral E illuminant).
    let xyz_to_rgb = mat3x3<f32>(2.3706743, -0.5138850, 0.0052982, -0.9000405, 1.4253036, -0.0146949,
                                 -0.4706338, 0.0885814, 1.0093968);

    // Vacuum outside.
    let eta1 = 1.0;
    let eta2 = max(fd.tf_ior, eta1);
    let schlick = fd.model == SHADEWIRE_FRESNEL_SCHLICK;
    let eta3 = select(fd.ior, shadewire_mx_f0_to_ior(fd.f0), schlick);
    let kappa3 = select(fd.extinction, vec3<f32>(0.0), schlick);
    let cos_theta_t = sqrt(1.0 - (1.0 - cos_theta * cos_theta) * (eta1 / eta2) * (eta1 / eta2));

    // First interface.
    var r12 = shadewire_mx_fresnel_dielectric_polarized(cos_theta, eta2 / eta1);
    if (cos_theta_t <= 0.0) {
        r12 = vec2<f32>(1.0);  // total internal reflection
    }
    let t121 = vec2<f32>(1.0) - r12;

    // Second interface.
    var r23p: vec3<f32>;
    var r23s: vec3<f32>;
    if (schlick) {
        let f = shadewire_mx_fresnel_hoffman_schlick(cos_theta_t, fd);
        r23p = 0.5 * f;
        r23s = 0.5 * f;
    } else {
        let r23 = shadewire_mx_fresnel_conductor_polarized(cos_theta_t, eta3 / eta2, kappa3 / eta2);
        r23p = r23.p;
        r23s = r23.s;
    }

    // Phase shift.
    let cos_b = cos(atan(eta2 / eta1));
    let phi21 = vec2<f32>(select(SHADEWIRE_PI, 0.0, cos_theta < cos_b), SHADEWIRE_PI);
    var phi23p: vec3<f32>;
    var phi23s: vec3<f32>;
    if (schlick) {
        phi23p = select(vec3<f32>(0.0), vec3<f32>(SHADEWIRE_PI), eta3 < vec3<f32>(eta2));
        phi23s = phi23p;
    } else {
        let phi23 = shadewire_mx_fresnel_conductor_phase_polarized(cos_theta_t, eta2, eta3, kappa3);
        phi23p = phi23.p;
        phi23s = phi23.s;
    }
    let r123p = max(sqrt(r12.x * r23p), vec3<f32>(0.0));
    let r123s = max(sqrt(r12.y * r23s), vec3<f32>(0.0));

    // Optical path difference.
    let dist_meters = fd.tf_thickness * 1.0e-9;
    let opd = 2.0 * eta2 * cos_theta_t * dist_meters;

    // Parallel polarization: the m = 0 term, then pairs of diracs for m > 0.
    var i = vec3<f32>(0.0);
    let rs = (t121.x * t121.x * r23p) / (vec3<f32>(1.0) - r12.x * r23p);
    i += r12.x + rs;
    var cm = rs - t121.x;
    for (var m = 1; m <= SHADEWIRE_AIRY_FRESNEL_ITERATIONS; m++) {
        cm *= r123p;
        let sm = 2.0 * shadewire_mx_eval_sensitivity(f32(m) * opd, f32(m) * (phi23p + vec3<f32>(phi21.x)));
        i += cm * sm;
    }

    // Perpendicular polarization.
    let rp = (t121.y * t121.y * r23s) / (vec3<f32>(1.0) - r12.y * r23s);
    i += r12.y + rp;
    cm = rp - t121.y;
    for (var m = 1; m <= SHADEWIRE_AIRY_FRESNEL_ITERATIONS; m++) {
        cm *= r123s;
        let sm = 2.0 * shadewire_mx_eval_sensitivity(f32(m) * opd, f32(m) * (phi23s + vec3<f32>(phi21.y)));
        i += cm * sm;
    }

    // Average the polarizations, back to RGB reflectance.
    return clamp(xyz_to_rgb * (0.5 * i), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn shadewire_mx_init_fresnel_dielectric(ior: f32, tf_thickness: f32, tf_ior: f32) -> ShadewireFresnel {
    return ShadewireFresnel(SHADEWIRE_FRESNEL_DIELECTRIC, tf_thickness > 0.0, vec3<f32>(ior), vec3<f32>(0.0),
                            vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0), 0.0, tf_thickness, tf_ior);
}

fn shadewire_mx_init_fresnel_conductor(ior: vec3<f32>, extinction: vec3<f32>, tf_thickness: f32, tf_ior: f32) -> ShadewireFresnel {
    return ShadewireFresnel(SHADEWIRE_FRESNEL_CONDUCTOR, tf_thickness > 0.0, ior, extinction,
                            vec3<f32>(0.0), vec3<f32>(0.0), vec3<f32>(0.0), 0.0, tf_thickness, tf_ior);
}

fn shadewire_mx_init_fresnel_schlick(f0: vec3<f32>, f82: vec3<f32>, f90: vec3<f32>, exponent: f32, tf_thickness: f32, tf_ior: f32) -> ShadewireFresnel {
    return ShadewireFresnel(SHADEWIRE_FRESNEL_SCHLICK, tf_thickness > 0.0, vec3<f32>(0.0), vec3<f32>(0.0),
                            f0, f82, f90, exponent, tf_thickness, tf_ior);
}

fn shadewire_mx_compute_fresnel(cos_theta: f32, fd: ShadewireFresnel) -> vec3<f32> {
    if (fd.airy) {
        return shadewire_mx_fresnel_airy(cos_theta, fd);
    } else if (fd.model == SHADEWIRE_FRESNEL_DIELECTRIC) {
        return vec3<f32>(shadewire_mx_fresnel_dielectric(cos_theta, fd.ior.x));
    } else if (fd.model == SHADEWIRE_FRESNEL_CONDUCTOR) {
        return shadewire_mx_fresnel_conductor(cos_theta, fd.ior, fd.extinction);
    }
    return shadewire_mx_fresnel_hoffman_schlick(cos_theta, fd);
}

// Directional albedo for each Fresnel model: what the lobe reflects of a uniform white environment.
fn shadewire_mx_ggx_dir_albedo_fresnel(n_dot_v: f32, alpha: f32, fd: ShadewireFresnel) -> vec3<f32> {
    if (fd.airy) {
        // A blend between mirror and rougher cases keeps the angular color variation at low roughness.
        let mirror_dir_albedo = shadewire_mx_compute_fresnel(n_dot_v, fd);
        let f0 = shadewire_mx_fresnel_airy(1.0, fd);
        let rough_dir_albedo = shadewire_mx_ggx_dir_albedo(n_dot_v, alpha, f0, vec3<f32>(1.0));
        return mix(mirror_dir_albedo, rough_dir_albedo, vec3<f32>(sqrt(alpha)));
    } else if (fd.model == SHADEWIRE_FRESNEL_DIELECTRIC) {
        return shadewire_mx_ggx_dir_albedo(n_dot_v, alpha, vec3<f32>(shadewire_mx_ior_to_f0(fd.ior.x)), vec3<f32>(1.0));
    } else if (fd.model == SHADEWIRE_FRESNEL_CONDUCTOR) {
        let f0 = shadewire_mx_fresnel_conductor(1.0, fd.ior, fd.extinction);
        return shadewire_mx_ggx_dir_albedo(n_dot_v, alpha, f0, vec3<f32>(1.0));
    }
    return shadewire_mx_ggx_dir_albedo(n_dot_v, alpha, fd.f0, fd.f90);
}

// The GGX response toward v of light from l (D F G comp / 4 NdotV: NdotL cancels), and the energy compensation.
struct ShadewireGgx {
    response: vec3<f32>,
    comp: vec3<f32>,
}

fn shadewire_mx_ggx_light(l: vec3<f32>, v: vec3<f32>, n: vec3<f32>, tangent: vec3<f32>, alpha: vec2<f32>, fd: ShadewireFresnel) -> ShadewireGgx {
    var x = tangent - dot(tangent, n) * n;
    // A missing or degenerate tangent: any one will do for an isotropic lobe.
    x = select(normalize(x), shadewire_mx_orthonormal_basis(n)[0], dot(x, x) < SHADEWIRE_EPS);
    let y = cross(n, x);
    let h = normalize(l + v);
    let n_dot_v = clamp(dot(n, v), SHADEWIRE_EPS, 1.0);
    let n_dot_l = clamp(dot(n, l), SHADEWIRE_EPS, 1.0);
    let v_dot_h = clamp(dot(v, h), SHADEWIRE_EPS, 1.0);
    let ht = vec3<f32>(dot(h, x), dot(h, y), dot(h, n));
    let avg_alpha = shadewire_mx_average_alpha(alpha);
    let f = shadewire_mx_compute_fresnel(v_dot_h, fd);
    let d = shadewire_mx_ggx_ndf(ht, alpha);
    let g = shadewire_mx_ggx_smith_g2(n_dot_l, n_dot_v, avg_alpha);
    let comp = shadewire_mx_ggx_energy_compensation(n_dot_v, avg_alpha, f);
    return ShadewireGgx(d * f * g * comp / (4.0 * n_dot_v), comp);
}

// ------------------------------------------------------------------------------------------------- lobes

// `scatter_mode`: 0 reflects (R), 1 only transmits (T), 2 both (RT).
fn shadewire_dielectric_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, tint: vec3<f32>, ior: f32, roughness: vec2<f32>, retroreflective: bool, thinfilm_thickness: f32, thinfilm_ior: f32, normal: vec3<f32>, tangent: vec3<f32>, scatter_mode: i32) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS || scatter_mode == 1) {
        return shadewire_bsdf_none();
    }
    let vr = select(v, reflect(-v, normal), retroreflective);
    let n = shadewire_mx_forward_facing_normal(normal, vr);
    let n_dot_v = clamp(dot(n, vr), SHADEWIRE_EPS, 1.0);
    let fd = shadewire_mx_init_fresnel_dielectric(ior, thinfilm_thickness, thinfilm_ior);
    let alpha = clamp(roughness, vec2<f32>(SHADEWIRE_EPS), vec2<f32>(1.0));
    let avg_alpha = shadewire_mx_average_alpha(alpha);
    let ggx = shadewire_mx_ggx_light(l, vr, n, tangent, alpha, fd);
    let dir_albedo = shadewire_mx_ggx_dir_albedo(n_dot_v, avg_alpha, vec3<f32>(shadewire_mx_ior_to_f0(ior)), vec3<f32>(1.0)) * ggx.comp;
    return ShadewireBsdf(ggx.response * max(tint, vec3<f32>(0.0)) * weight, 1.0 - dir_albedo * weight);
}

fn shadewire_dielectric_bsdf_albedo(v: vec3<f32>, weight: f32, tint: vec3<f32>, ior: f32, roughness: vec2<f32>, retroreflective: bool, thinfilm_thickness: f32, thinfilm_ior: f32, normal: vec3<f32>, tangent: vec3<f32>, scatter_mode: i32) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS || scatter_mode == 1) {
        return shadewire_bsdf_none();
    }
    let vr = select(v, reflect(-v, normal), retroreflective);
    let n = shadewire_mx_forward_facing_normal(normal, vr);
    let n_dot_v = clamp(dot(n, vr), SHADEWIRE_EPS, 1.0);
    let fd = shadewire_mx_init_fresnel_dielectric(ior, thinfilm_thickness, thinfilm_ior);
    let alpha = clamp(roughness, vec2<f32>(SHADEWIRE_EPS), vec2<f32>(1.0));
    let avg_alpha = shadewire_mx_average_alpha(alpha);
    let comp = shadewire_mx_ggx_energy_compensation(n_dot_v, avg_alpha, shadewire_mx_compute_fresnel(n_dot_v, fd));
    let dir_albedo = shadewire_mx_ggx_dir_albedo(n_dot_v, avg_alpha, vec3<f32>(shadewire_mx_ior_to_f0(ior)), vec3<f32>(1.0)) * comp;
    let li = shadewire_mx_ggx_dir_albedo_fresnel(n_dot_v, avg_alpha, fd);
    return ShadewireBsdf(li * max(tint, vec3<f32>(0.0)) * comp * weight, 1.0 - dir_albedo * weight);
}

fn shadewire_conductor_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, ior: vec3<f32>, extinction: vec3<f32>, roughness: vec2<f32>, retroreflective: bool, thinfilm_thickness: f32, thinfilm_ior: f32, normal: vec3<f32>, tangent: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let vr = select(v, reflect(-v, normal), retroreflective);
    let n = shadewire_mx_forward_facing_normal(normal, vr);
    let fd = shadewire_mx_init_fresnel_conductor(ior, extinction, thinfilm_thickness, thinfilm_ior);
    let alpha = clamp(roughness, vec2<f32>(SHADEWIRE_EPS), vec2<f32>(1.0));
    let ggx = shadewire_mx_ggx_light(l, vr, n, tangent, alpha, fd);
    return ShadewireBsdf(ggx.response * weight, vec3<f32>(0.0));
}

fn shadewire_conductor_bsdf_albedo(v: vec3<f32>, weight: f32, ior: vec3<f32>, extinction: vec3<f32>, roughness: vec2<f32>, retroreflective: bool, thinfilm_thickness: f32, thinfilm_ior: f32, normal: vec3<f32>, tangent: vec3<f32>) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS) {
        return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(0.0));
    }
    let vr = select(v, reflect(-v, normal), retroreflective);
    let n = shadewire_mx_forward_facing_normal(normal, vr);
    let n_dot_v = clamp(dot(n, vr), SHADEWIRE_EPS, 1.0);
    let fd = shadewire_mx_init_fresnel_conductor(ior, extinction, thinfilm_thickness, thinfilm_ior);
    let alpha = clamp(roughness, vec2<f32>(SHADEWIRE_EPS), vec2<f32>(1.0));
    let avg_alpha = shadewire_mx_average_alpha(alpha);
    let comp = shadewire_mx_ggx_energy_compensation(n_dot_v, avg_alpha, shadewire_mx_compute_fresnel(n_dot_v, fd));
    let li = shadewire_mx_ggx_dir_albedo_fresnel(n_dot_v, avg_alpha, fd);
    return ShadewireBsdf(li * comp * weight, vec3<f32>(0.0));
}

fn shadewire_generalized_schlick_bsdf_light(l: vec3<f32>, v: vec3<f32>, weight: f32, color0: vec3<f32>, color82: vec3<f32>, color90: vec3<f32>, exponent: f32, roughness: vec2<f32>, retroreflective: bool, thinfilm_thickness: f32, thinfilm_ior: f32, normal: vec3<f32>, tangent: vec3<f32>, scatter_mode: i32) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS || scatter_mode == 1) {
        return shadewire_bsdf_none();
    }
    let vr = select(v, reflect(-v, normal), retroreflective);
    let n = shadewire_mx_forward_facing_normal(normal, vr);
    let n_dot_v = clamp(dot(n, vr), SHADEWIRE_EPS, 1.0);
    let c0 = max(color0, vec3<f32>(0.0));
    let c90 = max(color90, vec3<f32>(0.0));
    let fd = shadewire_mx_init_fresnel_schlick(c0, max(color82, vec3<f32>(0.0)), c90, exponent, thinfilm_thickness, thinfilm_ior);
    let alpha = clamp(roughness, vec2<f32>(SHADEWIRE_EPS), vec2<f32>(1.0));
    let avg_alpha = shadewire_mx_average_alpha(alpha);
    let ggx = shadewire_mx_ggx_light(l, vr, n, tangent, alpha, fd);
    let dir_albedo = shadewire_mx_ggx_dir_albedo(n_dot_v, avg_alpha, c0, c90) * ggx.comp;
    let avg_dir_albedo = dot(dir_albedo, vec3<f32>(1.0 / 3.0));
    return ShadewireBsdf(ggx.response * weight, vec3<f32>(1.0 - avg_dir_albedo * weight));
}

fn shadewire_generalized_schlick_bsdf_albedo(v: vec3<f32>, weight: f32, color0: vec3<f32>, color82: vec3<f32>, color90: vec3<f32>, exponent: f32, roughness: vec2<f32>, retroreflective: bool, thinfilm_thickness: f32, thinfilm_ior: f32, normal: vec3<f32>, tangent: vec3<f32>, scatter_mode: i32) -> ShadewireBsdf {
    if (weight < SHADEWIRE_EPS || scatter_mode == 1) {
        return shadewire_bsdf_none();
    }
    let vr = select(v, reflect(-v, normal), retroreflective);
    let n = shadewire_mx_forward_facing_normal(normal, vr);
    let n_dot_v = clamp(dot(n, vr), SHADEWIRE_EPS, 1.0);
    let c0 = max(color0, vec3<f32>(0.0));
    let c90 = max(color90, vec3<f32>(0.0));
    let fd = shadewire_mx_init_fresnel_schlick(c0, max(color82, vec3<f32>(0.0)), c90, exponent, thinfilm_thickness, thinfilm_ior);
    let alpha = clamp(roughness, vec2<f32>(SHADEWIRE_EPS), vec2<f32>(1.0));
    let avg_alpha = shadewire_mx_average_alpha(alpha);
    let comp = shadewire_mx_ggx_energy_compensation(n_dot_v, avg_alpha, shadewire_mx_compute_fresnel(n_dot_v, fd));
    let dir_albedo = shadewire_mx_ggx_dir_albedo(n_dot_v, avg_alpha, c0, c90) * comp;
    let avg_dir_albedo = dot(dir_albedo, vec3<f32>(1.0 / 3.0));
    let li = shadewire_mx_ggx_dir_albedo_fresnel(n_dot_v, avg_alpha, fd);
    return ShadewireBsdf(li * comp * weight, vec3<f32>(1.0 - avg_dir_albedo * weight));
}
