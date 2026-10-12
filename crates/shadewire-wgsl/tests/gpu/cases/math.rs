//! Math family: every op of `shadewire::ops::math`, float and color3 with a float second argument.

use super::{Case, EXACT, TRANSCENDENTAL, binary, op_case, ternary, triple, unary};
use shadewire::{Op, Type, Value};

const INPUTS: [f32; 7] = [-2.5, -1.0, -0.3, 0.0, 0.3, 1.0, 2.5];
const F: Type = Type::Float;
const C3: Type = Type::Color3;

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    unary_ops(&mut out);
    binary_ops(&mut out);
    ternary_ops(&mut out);
    integer_ops(&mut out);
    out
}

fn unary_ops(out: &mut Vec<Case>) {
    let all: Vec<f32> = INPUTS.to_vec();
    let non_negative: Vec<f32> = INPUTS.iter().copied().filter(|x| *x >= 0.0).collect();
    let positive: Vec<f32> = INPUTS.iter().copied().filter(|x| *x > 0.0).collect();
    let ops: [(Op, &Vec<f32>, f32); 9] = [
        (Op::Absval, &all, EXACT),
        (Op::Sign, &all, EXACT),
        (Op::Floor, &all, EXACT),
        (Op::Ceil, &all, EXACT),
        (Op::Round, &all, EXACT),
        (Op::Fract, &all, EXACT),
        (Op::Sqrt, &non_negative, EXACT),
        (Op::Exp, &all, TRANSCENDENTAL),
        (Op::Ln, &positive, TRANSCENDENTAL),
    ];
    for (op, inputs, tolerance) in ops {
        for (i, x) in inputs.iter().enumerate() {
            out.push(unary("math", op, F, Value::Float(*x), tolerance));
            out.push(unary("math", op, C3, triple(inputs, i), tolerance));
        }
    }
}

/// Whether `op(a, b)` is in the domain the parity test covers.
fn binary_domain(op: Op, a: f32, b: f32) -> bool {
    match op {
        Op::Divide | Op::Modulo => b != 0.0,
        // WGSL pow, like GLSL pow, is undefined for a negative base and for 0^0.
        Op::Power => a > 0.0 || (a == 0.0 && b > 0.0),
        // sign(0) * pow(0, b) is 0 * inf for b < 0 and pow(0, 0) for b == 0.
        Op::SafePower => a != 0.0 || b > 0.0,
        _ => true,
    }
}

fn binary_ops(out: &mut Vec<Case>) {
    let ops: [(Op, f32); 10] = [
        (Op::Add, EXACT),
        (Op::Subtract, EXACT),
        (Op::Multiply, EXACT),
        (Op::Divide, EXACT),
        (Op::Modulo, EXACT),
        (Op::Invert, EXACT),
        (Op::Min, EXACT),
        (Op::Max, EXACT),
        (Op::Power, TRANSCENDENTAL),
        (Op::SafePower, TRANSCENDENTAL),
    ];
    for (op, tolerance) in ops {
        for a in INPUTS {
            for b in INPUTS {
                if binary_domain(op, a, b) {
                    out.push(binary("math", op, F, Value::Float(a), Value::Float(b), tolerance));
                }
            }
        }
        for i in 0..INPUTS.len() {
            let Value::Color3(rgb) = triple(&INPUTS, i) else { unreachable!() };
            for b in INPUTS {
                if rgb.iter().all(|a| binary_domain(op, *a, b)) {
                    out.push(binary("math", op, C3, Value::Color3(rgb), Value::Float(b), tolerance));
                }
            }
        }
    }
    // Power over color3 bases that are all in its domain.
    for i in 0..3 {
        for b in INPUTS {
            out.push(binary("math", Op::Power, C3, triple(&[0.3, 1.0, 2.5], i), Value::Float(b), TRANSCENDENTAL));
        }
    }
}

fn ternary_ops(out: &mut Vec<Case>) {
    let bounds = [(-1.0, 1.0), (0.0, 0.3), (-2.5, -0.3), (0.3, 0.3)];
    for (i, x) in INPUTS.iter().enumerate() {
        for (low, high) in bounds {
            let (low, high) = (Value::Float(low), Value::Float(high));
            out.push(ternary("math", Op::Clamp, F, Value::Float(*x), low.clone(), high.clone(), EXACT));
            out.push(ternary("math", Op::Clamp, C3, triple(&INPUTS, i), low.clone(), high.clone(), EXACT));
            out.push(ternary("math", Op::Smoothstep, F, Value::Float(*x), low.clone(), high.clone(), EXACT));
            out.push(ternary("math", Op::Smoothstep, C3, triple(&INPUTS, i), low, high, EXACT));
        }
        for bg in [-1.0, 2.5] {
            for amount in [0.0, 0.3, 1.0, 2.5] {
                let args = (Value::Float(*x), Value::Float(bg), Value::Float(amount));
                out.push(ternary("math", Op::Mix, F, args.0, args.1, args.2, EXACT));
            }
        }
        let mix_c3 = [triple(&INPUTS, i), Value::Color3([0.1, 0.5, 0.9]), Value::Float(0.3)];
        out.push(op_case("math", Op::Mix, C3, &mix_c3, EXACT));
        let mix_per_channel = [triple(&INPUTS, i), Value::Float(1.0), triple(&INPUTS, i + 3)];
        out.push(op_case("math", Op::Mix, C3, &mix_per_channel, EXACT));
        let ranges = [(0.0, 1.0, -1.0, 1.0), (-1.0, 2.5, 0.3, 0.0), (2.5, -1.0, 1.0, 0.0)];
        for (in_low, in_high, out_low, out_high) in ranges {
            let mut args = vec![Value::Float(*x)];
            args.extend([in_low, in_high, out_low, out_high].map(Value::Float));
            out.push(op_case("math", Op::Remap, F, &args, EXACT));
            args[0] = triple(&INPUTS, i);
            out.push(op_case("math", Op::Remap, C3, &args, EXACT));
        }
    }
}

/// Integer results: the interpreter computes in f32 and converts, the emitter does the same.
fn integer_ops(out: &mut Vec<Case>) {
    let pairs = [(-7, 2), (7, -2), (9, 4), (-9, -4)];
    for op in [Op::Add, Op::Subtract, Op::Multiply, Op::Divide, Op::Modulo, Op::Min, Op::Max] {
        for (a, b) in pairs {
            out.push(binary("math", op, Type::Integer, Value::Integer(a), Value::Integer(b), EXACT));
        }
    }
    for x in [-3, 0, 5] {
        out.push(unary("math", Op::Absval, Type::Integer, Value::Integer(x), EXACT));
        out.push(unary("math", Op::Sign, Type::Integer, Value::Integer(x), EXACT));
    }
}
