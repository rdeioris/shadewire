use shadewire::{Op, Type};

use super::{Arg, cast, finish, first_f32, leading, work_type};

/// Mirrors `shadewire::ops::vector`. Operands keep their own width (the first
/// argument's); scalar forms are written out (`normalize(x)` is `x / abs(x)`).
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    if *op == Op::Rotate2d {
        let call = format!("shadewire_rotate_vector2({}, {})", leading(&args[0], 2), first_f32(&args[1]));
        return cast(&call, Type::Vector2, ty);
    }
    if args.len() == 2 && args[0].1.component_count() != args[1].1.component_count() {
        panic!("{op:?} of different widths: {:?} and {:?}", args[0].1, args[1].1);
    }
    let operand = work_type(args[0].1);
    let a: Vec<String> = args.iter().map(|(expr, from)| cast(expr, *from, operand)).collect();
    let scalar = operand.is_scalar();
    let expr = match op {
        Op::Normalize if scalar => format!("({0} / abs({0}))", a[0]),
        Op::Normalize => format!("normalize({})", a[0]),
        Op::Magnitude if scalar => format!("abs({})", a[0]),
        Op::Magnitude => format!("length({})", a[0]),
        Op::Distance if scalar => format!("abs({} - {})", a[0], a[1]),
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
