//! Vector family: normalize, magnitude, distance, dot and cross products over fixed vectors.

use super::{binary, unary, Case, EXACT};
use shadewire_ir::{Op, Type, Value};

const V3: Type = Type::Vector3;

pub fn cases() -> Vec<Case> {
    let pairs: [([f32; 3], [f32; 3]); 5] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([3.0, 0.0, 4.0], [-1.0, 2.0, 0.5]),
        ([0.3, -2.5, 1.0], [2.5, 0.3, -1.0]),
        ([-0.7, -0.2, 0.9], [-0.7, -0.2, 0.9]),
        ([10.0, -3.0, 0.25], [0.1, 0.2, -0.3]),
    ];
    let mut out = Vec::new();
    for (a, b) in pairs {
        let (a, b) = (Value::Vector3(a), Value::Vector3(b));
        out.push(unary("vector", Op::Normalize, V3, a.clone(), EXACT));
        out.push(unary("vector", Op::Magnitude, Type::Float, a.clone(), EXACT));
        out.push(binary("vector", Op::Distance, Type::Float, a.clone(), b.clone(), EXACT));
        out.push(binary("vector", Op::DotProduct, Type::Float, a.clone(), b.clone(), EXACT));
        out.push(binary("vector", Op::CrossProduct, V3, a, b, EXACT));
    }
    let v2 = Value::Vector2([0.6, -0.8]);
    let v4 = Value::Vector4([1.0, -2.0, 0.5, 2.0]);
    for v in [v2, v4] {
        out.push(unary("vector", Op::Normalize, v.ty(), v.clone(), EXACT));
        out.push(unary("vector", Op::Magnitude, Type::Float, v.clone(), EXACT));
        out.push(binary("vector", Op::DotProduct, Type::Float, v.clone(), v, EXACT));
    }
    out
}
