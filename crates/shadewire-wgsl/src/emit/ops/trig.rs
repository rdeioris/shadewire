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
        Op::Atan2 => format!("atan2({}, {})", a[0], a[1]),
        other => panic!("{other:?} is not a trig op"),
    };
    finish(expr, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire::{Op, Type};

    #[test]
    fn builtins() {
        assert_eq!(emit(&Op::Sin, &args(&[Type::Float]), Type::Float), "sin(n0)");
        assert_eq!(emit(&Op::Acos, &args(&[Type::Vector3]), Type::Vector3), "acos(n0)");
    }

    #[test]
    fn atan2_takes_y_first() {
        assert_eq!(emit(&Op::Atan2, &args(&[Type::Float, Type::Float]), Type::Float), "atan2(n0, n1)");
        assert_eq!(emit(&Op::Atan2, &args(&[Type::Vector2, Type::Float]), Type::Vector2), "atan2(n0, vec2<f32>(n1))");
    }
}
