use shadewire_ir::{Op, Type};

use super::{cast, finish, first_f32, leading, work_type, Arg};

/// Mirrors `shadewire_ir::ops::vector`. Operands keep their own width (the first
/// argument's); WGSL has no scalar `normalize` or `dot`, so those are written out.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    if *op == Op::Rotate2d {
        let call = format!("mx_rotate_vector2({}, {})", leading(&args[0], 2), first_f32(&args[1]));
        return cast(&call, Type::Vector2, ty);
    }
    if args.len() == 2 && args[0].1.component_count() != args[1].1.component_count() {
        panic!("{op:?} of different widths: {:?} and {:?}", args[0].1, args[1].1);
    }
    let operand = work_type(args[0].1);
    let a: Vec<String> = args.iter().map(|(expr, from)| cast(expr, *from, operand)).collect();
    let scalar = operand.is_scalar();
    let expr = match op {
        Op::Normalize if scalar => format!("({0} / length({0}))", a[0]),
        Op::Normalize => format!("normalize({})", a[0]),
        Op::Magnitude => format!("length({})", a[0]),
        Op::Distance => format!("distance({}, {})", a[0], a[1]),
        Op::DotProduct if scalar => format!("({} * {})", a[0], a[1]),
        Op::DotProduct => format!("dot({}, {})", a[0], a[1]),
        Op::CrossProduct => format!("cross({}, {})", a[0], a[1]),
        other => panic!("{other:?} is not a vector op"),
    };
    let result = match op {
        Op::Normalize | Op::CrossProduct => operand,
        _ => Type::Float,
    };
    finish(cast(&expr, result, work_type(ty)), ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    const V3: Type = Type::Vector3;

    #[test]
    fn builtins() {
        assert_eq!(emit(&Op::Normalize, &args(&[V3]), V3), "normalize(n0)");
        assert_eq!(emit(&Op::Magnitude, &args(&[V3]), Type::Float), "length(n0)");
        assert_eq!(emit(&Op::Distance, &args(&[V3, V3]), Type::Float), "distance(n0, n1)");
        assert_eq!(emit(&Op::DotProduct, &args(&[V3, Type::Color3]), Type::Float), "dot(n0, n1)");
        assert_eq!(emit(&Op::CrossProduct, &args(&[V3, V3]), V3), "cross(n0, n1)");
    }

    #[test]
    fn rotate2d_calls_the_library() {
        assert_eq!(emit(&Op::Rotate2d, &args(&[Type::Vector2, Type::Float]), Type::Vector2), "mx_rotate_vector2(n0, n1)");
        assert_eq!(emit(&Op::Rotate2d, &args(&[Type::Vector2, Type::Integer]), Type::Vector2), "mx_rotate_vector2(n0, f32(n1))");
    }

    #[test]
    #[should_panic(expected = "different widths")]
    fn dot_of_different_widths_panics() {
        emit(&Op::DotProduct, &args(&[V3, Type::Vector2]), Type::Float);
    }

    #[test]
    #[should_panic(expected = "different widths")]
    fn distance_of_different_widths_panics() {
        emit(&Op::Distance, &args(&[Type::Vector4, Type::Float]), Type::Float);
    }

    #[test]
    #[should_panic(expected = "different widths")]
    fn cross_of_different_widths_panics() {
        emit(&Op::CrossProduct, &args(&[V3, Type::Color4]), V3);
    }
}
