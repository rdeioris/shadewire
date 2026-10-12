use shadewire::{Op, Type};

use super::{Arg, finish, wide};

/// Mirrors `shadewire::ops::trig`, componentwise over the node type.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let a = wide(args, ty);
    let expr = match op {
        Op::Sin => format!("sin({})", a[0]),
        Op::Cos => format!("cos({})", a[0]),
        Op::Tan => format!("tan({})", a[0]),
        Op::Asin => format!("asin({})", a[0]),
        Op::Acos => format!("acos({})", a[0]),
        // Arguments are iny, inx.
        Op::Atan2 => format!("atan({}, {})", a[0], a[1]),
        other => panic!("{other:?} is not a trig op"),
    };
    finish(expr, ty)
}
