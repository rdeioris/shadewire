//! Noise nodes. The node formulas follow the genglsl implementation files named in
//! stdlib_genglsl_impl.mtlx (mx_noise2d_*.glsl, mx_fractal2d_*.glsl, mx_fractal3d_*.glsl, mx_cellnoise*.glsl,
//! mx_worleynoise*.glsl); color and float-amplitude nodedefs are nodegraphs over these.
//! Arguments arrive in `catalog::input_names` order.
//!
//! Ported from MaterialX libraries/stdlib/genglsl/lib/mx_noise.glsl (Apache-2.0), itself derived
//! from OSL noise, Copyright (c) 2009-2010 Sony Pictures Imageworks Inc., et al., BSD-3-Clause.
//! See NOTICE.

pub mod basics;
pub mod cell;
pub mod fractal;
pub mod hash;
pub mod perlin;
pub mod worley;

use super::broadcast::broadcast;
use crate::{Op, Type, Value};

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    match op {
        Op::Noise2d => noise2d(&args[0], args[1].as_f32(), vec2(&args[2]), ty),
        Op::Noise3d => noise3d(&args[0], args[1].as_f32(), vec3(&args[2]), ty),
        Op::Fractal2d => {
            let (octaves, lacunarity, diminish) = (int(&args[1]), args[2].as_f32(), args[3].as_f32());
            fractal2d(&args[0], octaves, lacunarity, diminish, vec2(&args[4]), ty)
        }
        Op::Fractal3d => {
            let (octaves, lacunarity, diminish) = (int(&args[1]), args[2].as_f32(), args[3].as_f32());
            fractal3d(&args[0], octaves, lacunarity, diminish, vec3(&args[4]), ty)
        }
        Op::CellNoise2d => Value::Float(cell::cell_noise_float_2d(vec2(&args[0]))),
        Op::CellNoise3d => Value::Float(cell::cell_noise_float_3d(vec3(&args[0]))),
        Op::WorleyNoise2d => worleynoise2d(vec2(&args[0]), args[1].as_f32(), int(&args[2]), ty),
        Op::WorleyNoise3d => worleynoise3d(vec3(&args[0]), args[1].as_f32(), int(&args[2]), ty),
        other => panic!("{other:?} is not a noise op"),
    }
}

/// mx_noise2d_float / _vector2 / _vector3 / _vector4: `value * amplitude + pivot`.
fn noise2d(amplitude: &Value, pivot: f32, texcoord: [f32; 2], ty: Type) -> Value {
    let value = match ty.component_count() {
        1 => vec![perlin::perlin_noise_float_2d(texcoord)],
        2 => perlin::perlin_noise_vec3_2d(texcoord)[..2].to_vec(),
        3 => perlin::perlin_noise_vec3_2d(texcoord).to_vec(),
        _ => {
            let xyz = perlin::perlin_noise_vec3_2d(texcoord);
            let w = perlin::perlin_noise_float_2d([texcoord[0] + 19.0, texcoord[1] + 73.0]);
            vec![xyz[0], xyz[1], xyz[2], w]
        }
    };
    scale(&value, amplitude, pivot, ty)
}

/// mx_noise3d_float / _vector2 / _vector3 / _vector4: `value * amplitude + pivot`.
fn noise3d(amplitude: &Value, pivot: f32, position: [f32; 3], ty: Type) -> Value {
    let value = match ty.component_count() {
        1 => vec![perlin::perlin_noise_float_3d(position)],
        2 => perlin::perlin_noise_vec3_3d(position)[..2].to_vec(),
        3 => perlin::perlin_noise_vec3_3d(position).to_vec(),
        _ => {
            let xyz = perlin::perlin_noise_vec3_3d(position);
            let w = perlin::perlin_noise_float_3d([position[0] + 19.0, position[1] + 73.0, position[2] + 29.0]);
            vec![xyz[0], xyz[1], xyz[2], w]
        }
    };
    scale(&value, amplitude, pivot, ty)
}

/// mx_fractal2d_float / _vector2 / _vector3 / _vector4: `value * amplitude`.
fn fractal2d(amplitude: &Value, octaves: i32, lacunarity: f32, diminish: f32, texcoord: [f32; 2], ty: Type) -> Value {
    let value = match ty.component_count() {
        1 => vec![fractal::fractal_noise_float_2d(texcoord, octaves, lacunarity, diminish)],
        2 => fractal::fractal_noise_vec2_2d(texcoord, octaves, lacunarity, diminish).to_vec(),
        3 => fractal::fractal_noise_vec3_2d(texcoord, octaves, lacunarity, diminish).to_vec(),
        _ => fractal::fractal_noise_vec4_2d(texcoord, octaves, lacunarity, diminish).to_vec(),
    };
    scale(&value, amplitude, 0.0, ty)
}

/// mx_fractal3d_float / _vector2 / _vector3 / _vector4: `value * amplitude`.
fn fractal3d(amplitude: &Value, octaves: i32, lacunarity: f32, diminish: f32, position: [f32; 3], ty: Type) -> Value {
    let value = match ty.component_count() {
        1 => vec![fractal::fractal_noise_float_3d(position, octaves, lacunarity, diminish)],
        2 => fractal::fractal_noise_vec2_3d(position, octaves, lacunarity, diminish).to_vec(),
        3 => fractal::fractal_noise_vec3_3d(position, octaves, lacunarity, diminish).to_vec(),
        _ => fractal::fractal_noise_vec4_3d(position, octaves, lacunarity, diminish).to_vec(),
    };
    scale(&value, amplitude, 0.0, ty)
}

/// mx_worleynoise2d_float / _vector2 / _vector3: metric is always 0 (Euclidean).
fn worleynoise2d(texcoord: [f32; 2], jitter: f32, style: i32, ty: Type) -> Value {
    let value = match ty.component_count() {
        1 => vec![worley::worley_noise_float_2d(texcoord, jitter, style, 0)],
        2 => worley::worley_noise_vec2_2d(texcoord, jitter, style, 0).to_vec(),
        _ => worley::worley_noise_vec3_2d(texcoord, jitter, style, 0).to_vec(),
    };
    Value::from_floats(ty, &value)
}

/// mx_worleynoise3d_float / _vector2 / _vector3: metric is always 0 (Euclidean).
fn worleynoise3d(position: [f32; 3], jitter: f32, style: i32, ty: Type) -> Value {
    let value = match ty.component_count() {
        1 => vec![worley::worley_noise_float_3d(position, jitter, style, 0)],
        2 => worley::worley_noise_vec2_3d(position, jitter, style, 0).to_vec(),
        _ => worley::worley_noise_vec3_3d(position, jitter, style, 0).to_vec(),
    };
    Value::from_floats(ty, &value)
}

/// `value * amplitude + pivot` per component, amplitude broadcast to the result type.
fn scale(value: &[f32], amplitude: &Value, pivot: f32, ty: Type) -> Value {
    let amp = broadcast(amplitude, ty).floats();
    let out: Vec<f32> = value.iter().zip(amp).map(|(v, a)| v * a + pivot).collect();
    Value::from_floats(ty, &out)
}

fn vec2(v: &Value) -> [f32; 2] {
    let f = v.floats();
    [f[0], f[1]]
}

fn vec3(v: &Value) -> [f32; 3] {
    let f = v.floats();
    [f[0], f[1], f[2]]
}

/// Integer and enum inputs (octaves, style) arrive as `Value::Integer`.
fn int(v: &Value) -> i32 {
    match v {
        Value::Integer(i) => *i,
        other => other.as_f32() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic_and_spreads() {
        assert_eq!(hash::hash_int_2(1, 2), hash::hash_int_2(1, 2));
        assert_ne!(hash::hash_int_2(1, 2), hash::hash_int_2(2, 1));
    }

    #[test]
    fn perlin_is_zero_on_lattice_points() {
        assert!(perlin::perlin_noise_float_2d([3.0, -2.0]).abs() < 1e-6);
    }

    #[test]
    fn perlin_stays_in_range() {
        for i in 0..1000 {
            let p = [i as f32 * 0.137, i as f32 * 0.071];
            let n = perlin::perlin_noise_float_2d(p);
            assert!((-1.0..=1.0).contains(&n), "{n} at {p:?}");
        }
    }

    #[test]
    fn cell_noise_is_constant_inside_a_cell() {
        assert_eq!(cell::cell_noise_float_2d([0.1, 0.1]), cell::cell_noise_float_2d([0.9, 0.9]));
    }

    fn v2(x: f32, y: f32) -> Value {
        Value::Vector2([x, y])
    }

    fn v3(x: f32, y: f32, z: f32) -> Value {
        Value::Vector3([x, y, z])
    }

    fn close(a: &Value, b: &Value) -> bool {
        a.ty() == b.ty() && a.floats().iter().zip(b.floats()).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn noise2d_on_a_lattice_point_is_the_pivot() {
        let args = [Value::Float(2.0), Value::Float(0.5), v2(3.0, -2.0)];
        assert_eq!(eval(&Op::Noise2d, &args, Type::Float), Value::Float(0.5));
    }

    #[test]
    fn noise2d_vector4_takes_w_from_the_offset_float_noise() {
        let p = [0.3, 0.7];
        let args = [Value::Vector4([1.0, 1.0, 1.0, 2.0]), Value::Float(0.25), v2(p[0], p[1])];
        let xyz = perlin::perlin_noise_vec3_2d(p);
        let w = perlin::perlin_noise_float_2d([p[0] + 19.0, p[1] + 73.0]);
        let want = Value::Vector4([xyz[0] + 0.25, xyz[1] + 0.25, xyz[2] + 0.25, w * 2.0 + 0.25]);
        assert!(close(&eval(&Op::Noise2d, &args, Type::Vector4), &want));
    }

    #[test]
    fn noise3d_vector2_is_the_xy_of_the_vec3_noise() {
        let p = [0.3, 0.7, 1.9];
        let args = [Value::Float(3.0), Value::Float(0.0), v3(p[0], p[1], p[2])];
        let xyz = perlin::perlin_noise_vec3_3d(p);
        let want = Value::Vector2([xyz[0] * 3.0, xyz[1] * 3.0]);
        assert!(close(&eval(&Op::Noise3d, &args, Type::Vector2), &want));
    }

    #[test]
    fn fractal3d_sums_octaves() {
        let p = [0.3, 0.7, 1.9];
        let args = [Value::Float(2.0), Value::Integer(2), Value::Float(2.0), Value::Float(0.5), v3(p[0], p[1], p[2])];
        let want = 2.0 * (perlin::perlin_noise_float_3d(p) + 0.5 * perlin::perlin_noise_float_3d([0.6, 1.4, 3.8]));
        assert!(close(&eval(&Op::Fractal3d, &args, Type::Float), &Value::Float(want)));
    }

    #[test]
    fn fractal2d_sums_octaves() {
        let p = [0.3, 0.7];
        let args = [Value::Float(2.0), Value::Integer(2), Value::Float(2.0), Value::Float(0.5), v2(p[0], p[1])];
        let want = 2.0 * (perlin::perlin_noise_float_2d(p) + 0.5 * perlin::perlin_noise_float_2d([0.6, 1.4]));
        assert!(close(&eval(&Op::Fractal2d, &args, Type::Float), &Value::Float(want)));
    }

    #[test]
    fn fractal2d_vector2_offsets_its_second_channel() {
        let p = [0.3, 0.7];
        let args = [Value::Float(1.0), Value::Integer(3), Value::Float(2.0), Value::Float(0.5), v2(p[0], p[1])];
        let want = fractal::fractal_noise_float_2d([p[0] + 19.0, p[1] + 193.0], 3, 2.0, 0.5);
        assert!((eval(&Op::Fractal2d, &args, Type::Vector2).floats()[1] - want).abs() < 1e-6);
    }

    #[test]
    fn fractal3d_on_a_lattice_point_is_zero() {
        let args = [Value::Float(1.0), Value::Integer(3), Value::Float(2.0), Value::Float(0.5), v3(1.0, 2.0, -3.0)];
        assert!(close(&eval(&Op::Fractal3d, &args, Type::Vector4), &Value::Vector4([0.0; 4])));
    }

    #[test]
    fn cellnoise3d_is_in_unit_range() {
        let out = eval(&Op::CellNoise3d, &[v3(4.2, -1.5, 0.3)], Type::Float);
        assert_eq!(out, Value::Float(cell::cell_noise_float_3d([4.0, -2.0, 0.0])));
        assert!((0.0..=1.0).contains(&out.as_f32()));
    }

    // With jitter 0 every feature point sits at its cell centre, so distances are known by hand.
    #[test]
    fn worley2d_without_jitter_measures_to_cell_centres() {
        let at = |x, y, ty| eval(&Op::WorleyNoise2d, &[v2(x, y), Value::Float(0.0), Value::Integer(0)], ty);
        let h = 0.5_f32.sqrt();
        assert!(close(&at(0.5, 0.5, Type::Float), &Value::Float(0.0)));
        assert!(close(&at(0.0, 0.0, Type::Float), &Value::Float(h)));
        assert!(close(&at(0.0, 0.0, Type::Vector2), &Value::Vector2([h, h])));
        assert!(close(&at(0.5, 0.5, Type::Vector3), &Value::Vector3([0.0, 1.0, 1.0])));
    }

    #[test]
    fn worley3d_without_jitter_measures_to_cell_centres() {
        let at = |p: Value, ty| eval(&Op::WorleyNoise3d, &[p, Value::Float(0.0), Value::Integer(0)], ty);
        assert!(close(&at(v3(0.5, 0.5, 0.5), Type::Float), &Value::Float(0.0)));
        assert!(close(&at(v3(0.0, 0.0, 0.0), Type::Float), &Value::Float(0.75_f32.sqrt())));
        assert!(close(&at(v3(0.5, 0.5, 0.5), Type::Vector2), &Value::Vector2([0.0, 1.0])));
    }

    #[test]
    fn worley_solid_style_reads_the_nearest_cell() {
        let out = eval(&Op::WorleyNoise2d, &[v2(0.3, 0.6), Value::Float(0.0), Value::Integer(1)], Type::Float);
        assert_eq!(out, Value::Float(cell::cell_noise_float_2d([0.0, 0.0])));
    }
}
