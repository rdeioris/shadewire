//! Noise family: every noise op at every result width over a 5x5 grid of positions
//! `0.37 * i - 1.1`, plus far-out and negative positions; cell noise over a denser grid
//! so many hashes go through bits_to_01's u32 -> f32 rounding.

use super::{Case, TRANSCENDENTAL, op_case};
use shadewire::{Op, Type, Value};

const WIDTHS: [Type; 4] = [Type::Float, Type::Vector2, Type::Vector3, Type::Vector4];

pub fn cases() -> Vec<Case> {
    let mut points: Vec<[f32; 2]> = Vec::new();
    for i in 0..5 {
        for j in 0..5 {
            points.push([grid(i), grid(j)]);
        }
    }
    points.extend([[1.0e4 + 0.37, -1.0e4 - 0.61], [-12345.67, 987.65], [65536.25, -32768.75], [-0.5, -1.0e4]]);
    let mut out = Vec::new();
    for p in &points {
        every_op(&mut out, *p);
    }
    // fractal2d on the grid's diagonal and the far-out points only: at every point, the extra inlined
    // octave loops overflow the D3D12 driver's compiler on this one-module shader.
    for p in (0..5).map(|i| [grid(i), grid(i)]).chain(points[25..].iter().copied()) {
        for ty in WIDTHS {
            let fractal =
                [Value::Float(1.0), Value::Integer(3), Value::Float(2.0), Value::Float(0.5), Value::Vector2(p)];
            out.push(op_case("noise", Op::Fractal2d, ty, &fractal, TRANSCENDENTAL));
        }
    }
    for i in 0..12 {
        for j in 0..12 {
            let (x, y) = (grid(i), grid(j) * 7.3);
            out.push(op_case("noise", Op::CellNoise2d, Type::Float, &[Value::Vector2([x, y])], TRANSCENDENTAL));
            let p3 = Value::Vector3([x * 3.1, y, -0.5 * x]);
            out.push(op_case("noise", Op::CellNoise3d, Type::Float, &[p3], TRANSCENDENTAL));
        }
    }
    out
}

fn grid(i: i32) -> f32 {
    0.37 * i as f32 - 1.1
}

fn every_op(out: &mut Vec<Case>, [x, y]: [f32; 2]) {
    let p2 = Value::Vector2([x, y]);
    let p3 = Value::Vector3([x, y, 0.5 * x]);
    let (amplitude, pivot) = (Value::Float(0.7), Value::Float(0.2));
    for ty in WIDTHS {
        out.push(op_case("noise", Op::Noise2d, ty, &[amplitude.clone(), pivot.clone(), p2.clone()], TRANSCENDENTAL));
        out.push(op_case("noise", Op::Noise3d, ty, &[amplitude.clone(), pivot.clone(), p3.clone()], TRANSCENDENTAL));
        let fractal = [Value::Float(1.0), Value::Integer(3), Value::Float(2.0), Value::Float(0.5), p3.clone()];
        out.push(op_case("noise", Op::Fractal3d, ty, &fractal, TRANSCENDENTAL));
    }
    out.push(op_case("noise", Op::CellNoise2d, Type::Float, std::slice::from_ref(&p2), TRANSCENDENTAL));
    out.push(op_case("noise", Op::CellNoise3d, Type::Float, std::slice::from_ref(&p3), TRANSCENDENTAL));
    for ty in [Type::Float, Type::Vector2, Type::Vector3] {
        for style in [0, 1] {
            let jitter = Value::Float(1.0);
            let a2 = [p2.clone(), jitter.clone(), Value::Integer(style)];
            out.push(op_case("noise", Op::WorleyNoise2d, ty, &a2, TRANSCENDENTAL));
            let a3 = [p3.clone(), jitter, Value::Integer(style)];
            out.push(op_case("noise", Op::WorleyNoise3d, ty, &a3, TRANSCENDENTAL));
        }
    }
}
