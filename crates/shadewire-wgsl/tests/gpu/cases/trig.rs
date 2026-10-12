//! Trig family: sin, cos, tan, asin, acos over [-1, 1], atan2 over sign combinations.

use super::{Case, TRANSCENDENTAL, binary, triple, unary};
use shadewire::{Op, Type, Value};

const INPUTS: [f32; 5] = [-1.0, -0.5, 0.0, 0.5, 1.0];

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for op in [Op::Sin, Op::Cos, Op::Tan, Op::Asin, Op::Acos] {
        for (i, x) in INPUTS.iter().enumerate() {
            out.push(unary("trig", op, Type::Float, Value::Float(*x), TRANSCENDENTAL));
            out.push(unary("trig", op, Type::Color3, triple(&INPUTS, i), TRANSCENDENTAL));
        }
    }
    // atan2(y, x) in every quadrant and on every axis; (0, 0) is undefined in WGSL.
    for y in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            if x == 0.0 && y == 0.0 {
                continue;
            }
            out.push(binary("trig", Op::Atan2, Type::Float, Value::Float(y), Value::Float(x), TRANSCENDENTAL));
        }
    }
    let y = Value::Vector3([1.0, -0.5, -1.0]);
    out.push(binary("trig", Op::Atan2, Type::Vector3, y, Value::Float(-0.5), TRANSCENDENTAL));
    out
}
