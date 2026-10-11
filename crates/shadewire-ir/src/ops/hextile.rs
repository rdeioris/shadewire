//! Hex tiling (Mikkelsen, "Practical Real-Time Hex-Tiling", JCGT 11(2), 2022), as
//! MaterialX's `mx_hextile_coord` and `mx_hextile_compute_blend_weights` compute it.
//! `hextiledimage` is lowered to these ops, three gradient samples and a blend.
//!
//! Coordinates and gradients are in MaterialX uv space (V up). The tiles are laid out
//! in the V-down space MaterialX's GLSL computes them in (`mx_transform_uv` with
//! `fileTextureVerticalFlip`): each op flips its coordinate in and its result out, so the
//! tile pattern is the MaterialX viewer's, whatever the host's `flip_v`.
//!
//! Precision limit: the shadertoy hash is float arithmetic on values near 50 (its input,
//! `fract(id * 0.1031)`, is a fresh value for almost every tile id, negative ids too). A fused
//! multiply-add, which drivers are free to use, moves a tile's random values by about 1e-3:
//! about 0.4 degrees of rotation and 1e-3 uv of offset. The interpreter and the GPU, and
//! MaterialX's own GLSL across drivers, agree only that far. Blend weights use no hash.

use crate::{Op, Type, Value};

const SKEW_X: f32 = -0.577_350_27;
const SKEW_Y: f32 = 1.154_700_5;

/// `sqrt(3.0) * 2.0`: the scale that keeps a tile the size of the image.
fn sqrt3_2() -> f32 {
    3.0f32.sqrt() * 2.0
}

fn fract(x: f32) -> f32 {
    x - x.floor()
}

fn mix(a: f32, b: f32, t: f32) -> f32 {
    a * (1.0 - t) + b * t
}

/// MaterialX uv (V up) to and from the V-down layout space; vectors negate V.
fn flip_point(p: [f32; 2]) -> [f32; 2] {
    [p[0], 1.0 - p[1]]
}

fn flip_vector(v: [f32; 2]) -> [f32; 2] {
    [v[0], -v[1]]
}

/// The three tile vertices around `coord` (layout space) and their barycentric weights.
struct Layout {
    ids: [[f32; 2]; 3],
    weights: [f32; 3],
}

fn layout(coord: [f32; 2]) -> Layout {
    let st = [coord[0] * sqrt3_2(), coord[1] * sqrt3_2()];
    let skewed = [st[0] + SKEW_X * st[1], SKEW_Y * st[1]];
    let (fx, fy) = (fract(skewed[0]), fract(skewed[1]));
    let z = 1.0 - fx - fy;
    let s = if -z < 0.0 { 0.0 } else { 1.0 };
    let s2 = 2.0 * s - 1.0;
    let weights = [-z * s2, s - fy * s2, s - fx * s2];
    let base = [skewed[0].floor(), skewed[1].floor()];
    let ids = [[base[0] + s, base[1] + s], [base[0] + s, base[1] + 1.0 - s], [base[0] + 1.0 - s, base[1] + s]];
    Layout { ids, weights }
}

/// The tile center of vertex `id`, back in unskewed layout space.
fn center(id: [f32; 2]) -> [f32; 2] {
    let p = [id[0] / sqrt3_2(), id[1] / sqrt3_2()];
    [p[0] + 0.5 * p[1], (1.0 / SKEW_Y) * p[1]]
}

/// mx_hextile_hash (https://www.shadertoy.com/view/4djSRW), on `id + 0.12345`.
fn hash(id: [f32; 2]) -> [f32; 2] {
    let p = [id[0] + 0.12345, id[1] + 0.12345];
    let p3 = [fract(p[0] * 0.1031), fract(p[1] * 0.1030), fract(p[0] * 0.0973)];
    let d = p3[0] * (p3[1] + 33.33) + p3[1] * (p3[2] + 33.33) + p3[2] * (p3[0] + 33.33);
    let p3 = [p3[0] + d, p3[1] + d, p3[2] + d];
    [fract((p3[0] + p3[1]) * p3[2]), fract((p3[0] + p3[2]) * p3[1])]
}

/// A tile's random rotation (sine, cosine) and scale.
struct Transform {
    sin: f32,
    cos: f32,
    scale: f32,
}

fn transform(rand: [f32; 2], rotation: f32, rotation_range: [f32; 2], scale: f32, scale_range: [f32; 2]) -> Transform {
    let to_radians = std::f32::consts::PI / 180.0;
    let angle = mix(rotation_range[0] * to_radians, rotation_range[1] * to_radians, rand[0] * rotation);
    let scale = mix(1.0, mix(scale_range[0], scale_range[1], rand[1]), scale);
    Transform { sin: angle.sin(), cos: angle.cos(), scale }
}

/// `mx_matrix_mul(v, mat2(c, -s, s, c)) / scale`.
fn rotate(v: [f32; 2], t: &Transform) -> [f32; 2] {
    [(t.cos * v[0] - t.sin * v[1]) / t.scale, (t.sin * v[0] + t.cos * v[1]) / t.scale]
}

fn vec2(value: &Value) -> [f32; 2] {
    let f = value.floats();
    [f[0], f[1]]
}

/// Tile `tile`'s coordinate: args coord, rotation, rotationrange, scale, scalerange, offset, offsetrange.
fn coord(tile: usize, args: &[Value]) -> [f32; 2] {
    let c = flip_point(vec2(&args[0]));
    let id = layout(c).ids[tile];
    let rand = hash(id);
    let t = transform(rand, args[1].as_f32(), vec2(&args[2]), args[3].as_f32(), vec2(&args[4]));
    let (offset, range) = (args[5].as_f32(), vec2(&args[6]));
    let shift = [mix(range[0], range[1], rand[0] * offset), mix(range[0], range[1], rand[1] * offset)];
    let ctr = center(id);
    let r = rotate([c[0] - ctr[0], c[1] - ctr[1]], &t);
    flip_point([r[0] + ctr[0] + shift[0], r[1] + ctr[1] + shift[1]])
}

/// A derivative of the coordinate mapped into tile `tile`: args grad, coord, rotation,
/// rotationrange, scale, scalerange.
fn grad(tile: usize, args: &[Value]) -> [f32; 2] {
    let id = layout(flip_point(vec2(&args[1]))).ids[tile];
    let t = transform(hash(id), args[2].as_f32(), vec2(&args[3]), args[4].as_f32(), vec2(&args[5]));
    flip_vector(rotate(flip_vector(vec2(&args[0])), &t))
}

/// Christophe Schlick's gain (Graphics Gems IV), as mx_schlick_gain.
fn gain(x: f32, r: f32) -> f32 {
    let rr = r.clamp(0.001, 0.999);
    let a = (1.0 / rr - 2.0) * (1.0 - 2.0 * x);
    if x < 0.5 { x / (a + 1.0) } else { (a - x) / (a - 1.0) }
}

/// mx_hextile_compute_blend_weights: args coord, luminance weights, falloff.
fn weights(args: &[Value]) -> [f32; 3] {
    let tile = layout(flip_point(vec2(&args[0]))).weights;
    let lum = args[1].floats();
    let falloff = args[2].as_f32();
    let mut w = [lum[0] * tile[0].powf(7.0), lum[1] * tile[1].powf(7.0), lum[2] * tile[2].powf(7.0)];
    let sum = w[0] + w[1] + w[2];
    w = [w[0] / sum, w[1] / sum, w[2] / sum];
    if falloff != 0.5 {
        w = [gain(w[0], falloff), gain(w[1], falloff), gain(w[2], falloff)];
        let sum = w[0] + w[1] + w[2];
        w = [w[0] / sum, w[1] / sum, w[2] / sum];
    }
    w
}

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::HextileCoord(tile) => Value::from_floats(ty, &coord(*tile as usize, args)),
        Op::HextileGrad(tile) => Value::from_floats(ty, &grad(*tile as usize, args)),
        Op::HextileWeights => Value::from_floats(ty, &weights(args)),
        other => panic!("{other:?} is not a hextile op"),
    }
}

#[cfg(test)]
mod tests {
    use super::eval;
    use crate::{Op, Type, Value};

    const V2: Type = Type::Vector2;

    fn close(ours: &[f32], theirs: &[f64]) {
        for (o, t) in ours.iter().zip(theirs) {
            assert!((*o as f64 - t).abs() < 1e-5, "{ours:?} vs {theirs:?}");
        }
    }

    /// Default nodedef ranges: rotation 0..360, scale 0.5..2, offset 0..1, all at 1.
    fn tile_args(coord: [f32; 2]) -> Vec<Value> {
        vec![
            Value::Vector2(coord),
            Value::Float(1.0),
            Value::Vector2([0.0, 360.0]),
            Value::Float(1.0),
            Value::Vector2([0.5, 2.0]),
            Value::Float(1.0),
            Value::Vector2([0.0, 1.0]),
        ]
    }

    /// Reference values: an f32 (numpy) transcription of mx_hextile.glsl at MaterialX uv
    /// (0.3, 0.8). In f64 the float hash moves the coordinates by 1e-4.
    #[test]
    fn coords_gradients_and_weights_match_the_glsl() {
        let args = tile_args([0.3, 0.8]);
        let coords: [[f64; 2]; 3] = [[0.3204127, 0.7556577], [1.3263965, 0.5700183], [0.5031606, 0.7446634]];
        let grads: [[f64; 2]; 3] = [[0.57147384, 0.5823125], [0.15773702, 0.70463926], [-0.788293, -0.3940481]];
        for tile in 0..3u8 {
            close(&eval(&Op::HextileCoord(tile), &args, V2).floats(), &coords[tile as usize]);
            let mut g = vec![Value::Vector2([0.25, 0.5])];
            g.extend_from_slice(&args[..6]);
            close(&eval(&Op::HextileGrad(tile), &g, V2).floats(), &grads[tile as usize]);
        }
        let blend = [Value::Vector2([0.3, 0.8]), Value::Vector3([0.2, 0.5, 0.9]), Value::Float(0.75)];
        close(&eval(&Op::HextileWeights, &blend, Type::Vector3).floats(), &[0.48230624, 0.013758476, 0.5039353]);
    }

    /// No randomness asked for: every tile is the coordinate itself, every gradient unchanged.
    #[test]
    fn zero_amounts_are_the_identity() {
        let mut args = tile_args([2.7, -1.3]);
        args[1] = Value::Float(0.0);
        args[3] = Value::Float(0.0);
        args[5] = Value::Float(0.0);
        for tile in 0..3u8 {
            close(&eval(&Op::HextileCoord(tile), &args, V2).floats(), &[2.7, -1.3]);
            let mut g = vec![Value::Vector2([0.25, -0.5])];
            g.extend_from_slice(&args[..6]);
            close(&eval(&Op::HextileGrad(tile), &g, V2).floats(), &[0.25, -0.5]);
        }
    }

    /// Flat luminance and the neutral falloff: the barycentric weights, sharpened, sum to one.
    #[test]
    fn weights_sum_to_one() {
        for coord in [[0.1, 0.1], [0.37, 0.92], [5.5, -3.25]] {
            let args = [Value::Vector2(coord), Value::Vector3([1.0; 3]), Value::Float(0.5)];
            let w = eval(&Op::HextileWeights, &args, Type::Vector3).floats();
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5 && w.iter().all(|x| *x >= 0.0), "{w:?}");
        }
    }
}
