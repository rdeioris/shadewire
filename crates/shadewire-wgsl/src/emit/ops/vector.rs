use shadewire_ir::{Op, Type};

use super::{cast, finish, work_type, Arg};

/// Mirrors `shadewire_ir::ops::vector`. Operands keep their own width (the first
/// argument's); WGSL has no scalar `normalize` or `dot`, so those are written out.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
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
    use shadewire_ir::{Op, Type};

    fn args(types: &[Type]) -> Vec<(String, Type)> {
        types.iter().enumerate().map(|(i, t)| (format!("n{i}"), *t)).collect()
    }

    const V3: Type = Type::Vector3;

    #[test]
    fn builtins() {
        assert_eq!(emit(&Op::Normalize, &args(&[V3]), V3), "normalize(n0)");
        assert_eq!(emit(&Op::Magnitude, &args(&[V3]), Type::Float), "length(n0)");
        assert_eq!(emit(&Op::Distance, &args(&[V3, V3]), Type::Float), "distance(n0, n1)");
        assert_eq!(emit(&Op::DotProduct, &args(&[V3, Type::Color3]), Type::Float), "dot(n0, n1)");
        assert_eq!(emit(&Op::CrossProduct, &args(&[V3, V3]), V3), "cross(n0, n1)");
    }
}
