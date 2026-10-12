//! Channel family: every extract of a color4, combine2/3/4, every convert pair.

use super::{Case, EXACT, op_case, unary};
use shadewire::{Op, Type, Value};

const TYPES: [Type; 8] = [
    Type::Boolean,
    Type::Integer,
    Type::Float,
    Type::Vector2,
    Type::Vector3,
    Type::Vector4,
    Type::Color3,
    Type::Color4,
];

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let c4 = Value::Color4([0.1, -0.2, 0.3, 0.9]);
    for i in 0..4 {
        out.push(unary("channel", Op::Extract(i), Type::Float, c4.clone(), EXACT));
    }
    out.push(unary("channel", Op::Extract(1), Type::Float, Value::Vector3([4.0, 5.0, 6.0]), EXACT));
    let combines: [(Type, Vec<Value>); 5] = [
        (Type::Vector2, vec![Value::Float(0.5), Value::Float(-1.5)]),
        (Type::Color3, vec![Value::Float(0.1), Value::Float(0.2), Value::Float(0.3)]),
        (Type::Vector4, vec![Value::Float(1.0), Value::Float(2.0), Value::Float(3.0), Value::Float(4.0)]),
        (Type::Color4, vec![Value::Color3([0.1, 0.2, 0.3]), Value::Float(0.7)]),
        (Type::Vector4, vec![Value::Vector2([0.1, 0.2]), Value::Vector2([0.3, 0.4])]),
    ];
    for (ty, args) in combines {
        out.push(op_case("channel", Op::Combine, ty, &args, EXACT));
    }
    for from in TYPES {
        for to in TYPES {
            if from != to {
                out.push(unary("channel", Op::Convert, to, sample(from), EXACT));
            }
        }
    }
    out
}

/// One value of each type: a negative fraction exercises integer truncation.
fn sample(ty: Type) -> Value {
    match ty {
        Type::Boolean => Value::Boolean(true),
        Type::Integer => Value::Integer(-3),
        Type::Float => Value::Float(-2.5),
        Type::Vector2 => Value::Vector2([-2.5, 0.75]),
        Type::Vector3 => Value::Vector3([1.75, -0.5, 2.0]),
        Type::Vector4 => Value::Vector4([0.6, 0.0, -1.0, 4.0]),
        Type::Color3 => Value::Color3([0.2, 0.4, 0.6]),
        Type::Color4 => Value::Color4([0.0, 0.3, 0.5, 0.8]),
        other => panic!("no sample for {other:?}"),
    }
}
