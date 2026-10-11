//! Logic family: comparisons, both branches of every if-op, switch over `which`, boolean ops.

use super::{binary, op_case, unary, Case, EXACT};
use shadewire_ir::{Op, Type, Value};

const B: Type = Type::Boolean;

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    // value1 above, equal to and below value2: every comparison and if-op takes both branches.
    let compares = [(2.0, 1.0), (1.0, 1.0), (-0.5, 0.5)];
    let (yes, no) = (Value::Color3([1.0, 0.25, 0.0]), Value::Color3([0.0, 0.5, 1.0]));
    for (a, b) in compares {
        let (a, b) = (Value::Float(a), Value::Float(b));
        for op in [Op::Greater, Op::GreaterEq, Op::Equal] {
            out.push(binary("logic", op, B, a.clone(), b.clone(), EXACT));
        }
        for op in [Op::IfGreater, Op::IfGreaterEq, Op::IfEqual] {
            out.push(op_case("logic", op, Type::Color3, &[a.clone(), b.clone(), yes.clone(), no.clone()], EXACT));
            let scalars = [a.clone(), b.clone(), Value::Float(3.0), Value::Float(-4.0)];
            out.push(op_case("logic", op, Type::Float, &scalars, EXACT));
        }
    }
    let ints = [Value::Integer(3), Value::Integer(-2)];
    out.push(binary("logic", Op::Greater, B, ints[0].clone(), ints[1].clone(), EXACT));
    out.push(binary("logic", Op::Equal, B, ints[0].clone(), ints[0].clone(), EXACT));
    for (a, b) in [([0.1, 0.2, 0.3], [0.1, 0.2, 0.3]), ([0.1, 0.2, 0.3], [0.1, 0.2, 0.4])] {
        out.push(binary("logic", Op::Equal, B, Value::Color3(a), Value::Color3(b), EXACT));
        let args = [Value::Color3(a), Value::Color3(b), Value::Float(1.0), Value::Float(2.0)];
        out.push(op_case("logic", Op::IfEqual, Type::Float, &args, EXACT));
    }
    switch_cases(&mut out);
    for a in [false, true] {
        out.push(unary("logic", Op::Not, B, Value::Boolean(a), EXACT));
        for b in [false, true] {
            out.push(binary("logic", Op::And, B, Value::Boolean(a), Value::Boolean(b), EXACT));
            out.push(binary("logic", Op::Or, B, Value::Boolean(a), Value::Boolean(b), EXACT));
        }
    }
    out
}

/// which < 1 picks in1, [k-1, k) picks in_k, which >= 10 gives zero.
fn switch_cases(out: &mut Vec<Case>) {
    for which in [-1.0, 0.0, 0.5, 3.0, 9.0, 12.0] {
        let mut floats: Vec<Value> = (1..=10).map(|k| Value::Float(k as f32 * 1.5)).collect();
        floats.push(Value::Float(which));
        out.push(op_case("logic", Op::Switch, Type::Float, &floats, EXACT));
        let mut colors: Vec<Value> = (1..=10).map(|k| Value::Color3([k as f32, -(k as f32), 0.5])).collect();
        colors.push(Value::Float(which));
        out.push(op_case("logic", Op::Switch, Type::Color3, &colors, EXACT));
    }
    let mut by_integer: Vec<Value> = (1..=10).map(|k| Value::Float(k as f32)).collect();
    by_integer.push(Value::Integer(3));
    out.push(op_case("logic", Op::Switch, Type::Float, &by_integer, EXACT));
}
