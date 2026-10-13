// Closure evaluation, shared part: ported from MaterialX pbrlib genglsl (lib/mx_microfacet.glsl and the layer,
// mix, add, multiply and EDF nodes). A lobe answers in one of two contexts: `_light` (the response to light
// arriving from L, cosine included, as MaterialX's reflection context) and `_albedo` (the response to a uniform
// white environment, as MaterialX's indirect context with a constant environment: the directional albedo).

const SHADEWIRE_PI: f32 = 3.14159265358979323846;
const SHADEWIRE_PI_INV: f32 = 0.31830988618379067154;
const SHADEWIRE_EPS: f32 = 1e-8;

// What a closure does with light: `response` is the light it scatters toward the viewer, `throughput` the
// fraction it lets through to a closure layered under it.
struct ShadewireBsdf {
    response: vec3<f32>,
    throughput: vec3<f32>,
}

fn shadewire_bsdf_none() -> ShadewireBsdf {
    return ShadewireBsdf(vec3<f32>(0.0), vec3<f32>(1.0));
}

fn shadewire_mx_pow5(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2 * x;
}

fn shadewire_mx_pow6(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2 * x2;
}

// Generalized Schlick Fresnel.
fn shadewire_mx_fresnel_schlick_float(cos_theta: f32, f0: f32, f90: f32) -> f32 {
    let x = clamp(1.0 - cos_theta, 0.0, 1.0);
    return mix(f0, f90, shadewire_mx_pow5(x));
}

// Generalized Schlick Fresnel with a variable exponent.
fn shadewire_mx_fresnel_schlick_exponent(cos_theta: f32, f0: vec3<f32>, f90: vec3<f32>, exponent: f32) -> vec3<f32> {
    let x = clamp(1.0 - cos_theta, 0.0, 1.0);
    return mix(f0, f90, vec3<f32>(pow(x, exponent)));
}

// The normal, flipped to face the viewer.
fn shadewire_mx_forward_facing_normal(n: vec3<f32>, v: vec3<f32>) -> vec3<f32> {
    return select(n, -n, dot(n, v) < 0.0);
}

// https://graphics.pixar.com/library/OrthonormalB/paper.pdf
fn shadewire_mx_orthonormal_basis(n: vec3<f32>) -> mat3x3<f32> {
    let s = select(1.0, -1.0, n.z < 0.0);
    let a = -1.0 / (s + n.z);
    let b = n.x * n.y * a;
    let x = vec3<f32>(1.0 + s * n.x * n.x * a, s * b, -s * n.x);
    let y = vec3<f32>(b, s + n.y * n.y * a, -n.y);
    return mat3x3<f32>(x, y, n);
}

fn shadewire_mx_cosine_hemisphere_pdf(cos_theta: f32) -> f32 {
    return max(cos_theta, 0.0) * SHADEWIRE_PI_INV;
}

// ------------------------------------------------------------------------------------------------- composition

// `top` over `base`: the base receives what the top lets through.
fn shadewire_layer_bsdf(top: ShadewireBsdf, base: ShadewireBsdf) -> ShadewireBsdf {
    return ShadewireBsdf(top.response + base.response * top.throughput, top.throughput * base.throughput);
}

fn shadewire_mix_bsdf(fg: ShadewireBsdf, bg: ShadewireBsdf, mix_value: f32) -> ShadewireBsdf {
    return ShadewireBsdf(mix(bg.response, fg.response, vec3<f32>(mix_value)),
                         mix(bg.throughput, fg.throughput, vec3<f32>(mix_value)));
}

// The albedos add: throughput = 1 - (albedo_1 + albedo_2) = throughput_1 + throughput_2 - 1.
fn shadewire_add_bsdf(in1: ShadewireBsdf, in2: ShadewireBsdf) -> ShadewireBsdf {
    return ShadewireBsdf(in1.response + in2.response, max(in1.throughput + in2.throughput - 1.0, vec3<f32>(0.0)));
}

// A float weight arrives splatted to a color.
fn shadewire_multiply_bsdf(in1: ShadewireBsdf, in2: vec3<f32>) -> ShadewireBsdf {
    return ShadewireBsdf(in1.response * clamp(in2, vec3<f32>(0.0), vec3<f32>(1.0)), in1.throughput);
}

// ------------------------------------------------------------------------------------------------- emission

fn shadewire_uniform_edf(color: vec3<f32>) -> vec3<f32> {
    return color;
}

// MaterialX has no reference implementation; this follows the specification: full intensity within
// `inner_angle` of the normal, falling to zero at `outer_angle` (no falloff when it is not the larger), in degrees.
fn shadewire_conical_edf(v: vec3<f32>, color: vec3<f32>, normal: vec3<f32>, inner_angle: f32, outer_angle: f32) -> vec3<f32> {
    let n_dot_v = dot(shadewire_mx_forward_facing_normal(normal, v), v);
    let cos_inner = cos(radians(inner_angle));
    if (outer_angle <= inner_angle) {
        return select(vec3<f32>(0.0), color, n_dot_v >= cos_inner);
    }
    return color * smoothstep(cos(radians(outer_angle)), cos_inner, n_dot_v);
}

// As MaterialX's GLSL: the measured profile (`file`) is not read.
fn shadewire_measured_edf(color: vec3<f32>) -> vec3<f32> {
    return color;
}

fn shadewire_generalized_schlick_edf(v: vec3<f32>, n: vec3<f32>, color0: vec3<f32>, color90: vec3<f32>, exponent: f32, base: vec3<f32>) -> vec3<f32> {
    let nf = shadewire_mx_forward_facing_normal(n, v);
    let n_dot_v = clamp(dot(nf, v), SHADEWIRE_EPS, 1.0);
    return base * shadewire_mx_fresnel_schlick_exponent(n_dot_v, color0, color90, exponent);
}
