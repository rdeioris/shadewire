//! Integer family: the bitwise ops and the wrapping integer multiply, on values past 2^24 where floats round.

use super::{Case, binary};
use shadewire::{Op, Type, Value};

const INPUTS: [i32; 5] = [-1_879_048_193, -7, 0, 150, 1_075_385_539];
const SHIFTS: [i32; 5] = [0, 1, 13, 31, 33];

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    // Integers compare as f32 within EXACT relative tolerance: the f32 nearest each i32 result.
    for op in [Op::BitwiseAnd, Op::BitwiseOr, Op::BitwiseXor, Op::Multiply] {
        for a in INPUTS {
            for b in INPUTS {
                out.push(binary("bitwise", op, Type::Integer, Value::Integer(a), Value::Integer(b), super::EXACT));
            }
        }
    }
    for op in [Op::ShiftLeft, Op::ShiftRight, Op::ShiftRightUnsigned] {
        for a in INPUTS {
            for s in SHIFTS {
                out.push(binary("bitwise", op, Type::Integer, Value::Integer(a), Value::Integer(s), super::EXACT));
            }
        }
    }
    out
}
