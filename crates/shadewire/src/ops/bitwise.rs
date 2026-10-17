//! Bitwise operators on two's-complement integers, for MDL's `&`, `|`, `^`, `<<`, `>>` and `>>>`.

use crate::{Op, Type, Value};

fn int(v: &Value) -> i32 {
    match v {
        Value::Integer(i) => *i,
        other => other.as_f32() as i32,
    }
}

pub fn eval(op: &Op, args: &[Value], ty: Type) -> Value {
    let (a, b) = (int(&args[0]), int(&args[1]));
    // wrapping_sh* take the amount modulo 32, like WGSL and HLSL do at run time.
    let r = match op {
        Op::BitwiseAnd => a & b,
        Op::BitwiseOr => a | b,
        Op::BitwiseXor => a ^ b,
        Op::ShiftLeft => a.wrapping_shl(b as u32),
        Op::ShiftRight => a.wrapping_shr(b as u32),
        Op::ShiftRightUnsigned => (a as u32).wrapping_shr(b as u32) as i32,
        other => panic!("{other:?} is not a bitwise op"),
    };
    debug_assert_eq!(ty, Type::Integer, "bitwise ops are integer-only");
    Value::Integer(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(op: Op, a: i32, b: i32) -> i32 {
        match eval(&op, &[Value::Integer(a), Value::Integer(b)], Type::Integer) {
            Value::Integer(i) => i,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn two_complement() {
        assert_eq!(run(Op::BitwiseAnd, -1, 0x0F0), 0x0F0);
        assert_eq!(run(Op::BitwiseOr, 0x100, 0x011), 0x111);
        assert_eq!(run(Op::BitwiseXor, -1, 5), !5);
        assert_eq!(run(Op::ShiftLeft, 0x4000_0001, 1), 0x8000_0002u32 as i32);
        assert_eq!(run(Op::ShiftRight, -8, 1), -4);
        assert_eq!(run(Op::ShiftRightUnsigned, -8, 1), 0x7FFF_FFFC);
    }

    #[test]
    fn shift_amounts_wrap_at_32() {
        assert_eq!(run(Op::ShiftLeft, 3, 33), 6);
        assert_eq!(run(Op::ShiftRightUnsigned, -1, 32), -1);
    }
}
