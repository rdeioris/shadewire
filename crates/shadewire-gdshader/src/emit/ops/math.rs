use shadewire::{Op, Type};

use super::{Arg, cast, finish, select, wide, work_type};

/// Mirrors `shadewire::ops::math`: every argument is broadcast to the node type
/// (the mix amount stays scalar, a vector scales by it) and integer results
/// are computed in float then converted, like the interpreter.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let a = wide(args, ty);
    let expr = match op {
        Op::Add => format!("({} + {})", a[0], a[1]),
        Op::Subtract => format!("({} - {})", a[0], a[1]),
        Op::Multiply => format!("({} * {})", a[0], a[1]),
        Op::Divide => format!("({} / {})", a[0], a[1]),
        Op::Modulo => format!("({0} - {1} * floor({0} / {1}))", a[0], a[1]),
        Op::Invert => format!("({} - {})", a[1], a[0]),
        Op::Absval => format!("abs({})", a[0]),
        Op::Sign => format!("sign({})", a[0]),
        Op::Floor => format!("floor({})", a[0]),
        Op::Ceil => format!("ceil({})", a[0]),
        // Half to even, like `round_ties_even`; GLSL `round` leaves halves to the driver.
        Op::Round => format!("roundEven({})", a[0]),
        Op::Fract => format!("({0} - floor({0}))", a[0]),
        // `pow` is what MaterialX genglsl emits: a negative base and 0^0 are undefined on the GPU.
        Op::Power => format!("pow({}, {})", a[0], a[1]),
        Op::SafePower => format!("(sign({0}) * pow(abs({0}), {1}))", a[0], a[1]),
        Op::Sqrt => format!("sqrt({})", a[0]),
        Op::Exp => format!("exp({})", a[0]),
        Op::Ln => format!("log({})", a[0]),
        Op::Min => format!("min({}, {})", a[0], a[1]),
        Op::Max => format!("max({}, {})", a[0], a[1]),
        // Written out rather than `clamp`, which is undefined when low > high.
        Op::Clamp => format!("min(max({}, {}), {})", a[0], a[1], a[2]),
        // The interpreter's `bg + (fg - bg) * mix`.
        Op::Mix => {
            let amount = if args[2].1.is_scalar() { cast(&args[2].0, args[2].1, Type::Float) } else { a[2].clone() };
            format!("({1} + ({0} - {1}) * {2})", a[0], a[1], amount)
        }
        // The interpreter's `mx_smoothstep`, written out rather than the builtin.
        Op::Smoothstep => smoothstep(&a[0], &a[1], &a[2], work_type(ty)),
        Op::Remap => format!("({3} + ({0} - {1}) * ({4} - {3}) / ({2} - {1}))", a[0], a[1], a[2], a[3], a[4]),
        other => panic!("{other:?} is not a math op"),
    };
    finish(expr, ty)
}

/// 0 at or below `low`, 1 at or above `high`, the cubic between; componentwise for vectors
/// (`mix` with a boolean vector picks per component).
fn smoothstep(x: &str, low: &str, high: &str, work: Type) -> String {
    let zero = cast("0.0", Type::Float, work);
    let one = cast("1.0", Type::Float, work);
    let t = format!("(({x} - {low}) / ({high} - {low}))");
    let inner = format!("({t} * {t} * (3.0 - 2.0 * {t}))");
    if work.is_scalar() {
        return select(&select(&inner, &zero, &format!("{x} <= {low}")), &one, &format!("{x} >= {high}"));
    }
    format!("mix(mix({inner}, {zero}, lessThanEqual({x}, {low})), {one}, greaterThanEqual({x}, {high}))")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::ops::args;

    #[test]
    fn smoothstep_scalar_and_vector() {
        let f = emit(&Op::Smoothstep, &args(&[Type::Float; 3]), Type::Float);
        assert_eq!(
            f,
            "(n0 >= n2 ? 1.0 : (n0 <= n1 ? 0.0 : (((n0 - n1) / (n2 - n1)) * ((n0 - n1) / (n2 - n1)) * (3.0 - 2.0 * ((n0 - n1) / (n2 - n1))))))"
        );
        let v = emit(&Op::Smoothstep, &args(&[Type::Color3, Type::Float, Type::Float]), Type::Color3);
        assert!(v.starts_with("mix(mix("), "{v}");
        assert!(v.contains("lessThanEqual(n0, vec3(n1))") && v.contains("greaterThanEqual(n0, vec3(n2))"), "{v}");
    }

    #[test]
    fn mix_keeps_a_scalar_amount() {
        let m = emit(&Op::Mix, &args(&[Type::Color3, Type::Color3, Type::Float]), Type::Color3);
        assert_eq!(m, "(n1 + (n0 - n1) * n2)");
    }

    #[test]
    fn round_is_half_to_even() {
        assert_eq!(emit(&Op::Round, &args(&[Type::Float]), Type::Float), "roundEven(n0)");
    }
}
