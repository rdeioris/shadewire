//! Normal map family: four encoded normals (one all-zero), a scalar and a vector2 scale,
//! in an orthonormal and in a skewed frame.

use super::{Case, EXACT, op_case};
use shadewire_ir::{Op, Type, Value};

pub fn cases() -> Vec<Case> {
    let encoded = [[0.5, 0.5, 1.0], [0.8, 0.3, 0.9], [0.0, 0.0, 0.0], [0.2, 0.7, 0.6]];
    let frames = [
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.267, 0.535, 0.802], [0.9, -0.3, 0.1], [-0.2, 0.7, -0.4]),
    ];
    let scales = [Value::Float(1.0), Value::Vector2([0.5, 2.0])];
    let mut out = Vec::new();
    for e in encoded {
        for (n, t, b) in frames {
            for scale in &scales {
                let args = [Value::Vector3(e), scale.clone(), Value::Vector3(n), Value::Vector3(t), Value::Vector3(b)];
                out.push(op_case("normal", Op::NormalMap, Type::Vector3, &args, EXACT));
            }
        }
    }
    out
}
