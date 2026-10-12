use shadewire::{Op, Type};

use super::{Arg, cast, finish, wide, work_type};

/// Mirrors `shadewire::ops::math`: every argument is broadcast to the node type
/// (the mix amount stays scalar, WGSL scales a vector by it) and integer results
/// are computed in f32 then converted, like the interpreter.
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
        // WGSL round is half to even, like `round_ties_even`.
        Op::Round => format!("round({})", a[0]),
        Op::Fract => format!("({0} - floor({0}))", a[0]),
        // WGSL `pow` is what MaterialX genglsl emits. A negative base and 0^0 are undefined
        // on the GPU (as in GLSL), so they may differ from Rust's `powf` there.
        Op::Power => format!("pow({}, {})", a[0], a[1]),
        // sign(0.0) is 0.0 in WGSL, as in the interpreter; `pow` as for Power.
        Op::SafePower => format!("(sign({0}) * pow(abs({0}), {1}))", a[0], a[1]),
        Op::Sqrt => format!("sqrt({})", a[0]),
        Op::Exp => format!("exp({})", a[0]),
        Op::Ln => format!("log({})", a[0]),
        Op::Min => format!("min({}, {})", a[0], a[1]),
        Op::Max => format!("max({}, {})", a[0], a[1]),
        // Written out rather than `clamp`, which is an error in WGSL constant folding when low > high.
        Op::Clamp => format!("min(max({}, {}), {})", a[0], a[1], a[2]),
        // The interpreter's `bg + (fg - bg) * mix`.
        Op::Mix => {
            let amount = if args[2].1.is_scalar() { cast(&args[2].0, args[2].1, Type::Float) } else { a[2].clone() };
            format!("({1} + ({0} - {1}) * {2})", a[0], a[1], amount)
        }
        // The interpreter's `mx_smoothstep`, written out rather than the builtin.
        Op::Smoothstep => {
            let zero = cast("0.0", Type::Float, work_type(ty));
            let one = cast("1.0", Type::Float, work_type(ty));
            let t = format!("(({0} - {1}) / ({2} - {1}))", a[0], a[1], a[2]);
            let inner = format!("({t} * {t} * (3.0 - 2.0 * {t}))");
            format!("select(select({inner}, {zero}, {0} <= {1}), {one}, {0} >= {2})", a[0], a[1], a[2])
        }
        Op::Remap => format!("({3} + ({0} - {1}) * ({4} - {3}) / ({2} - {1}))", a[0], a[1], a[2], a[3], a[4]),
        other => panic!("{other:?} is not a math op"),
    };
    finish(expr, ty)
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire::{Op, Type};

    const F: Type = Type::Float;
    const C3: Type = Type::Color3;

    #[test]
    fn arithmetic() {
        assert_eq!(emit(&Op::Modulo, &args(&[F, F]), F), "(n0 - n1 * floor(n0 / n1))");
        assert_eq!(emit(&Op::Add, &args(&[C3, F]), C3), "(n0 + vec3<f32>(n1))");
        assert_eq!(emit(&Op::Divide, &args(&[F, F]), F), "(n0 / n1)");
        assert_eq!(emit(&Op::Invert, &args(&[F, F]), F), "(n1 - n0)");
        assert_eq!(emit(&Op::Fract, &args(&[F]), F), "(n0 - floor(n0))");
        assert_eq!(emit(&Op::Ln, &args(&[F]), F), "log(n0)");
        assert_eq!(emit(&Op::Round, &args(&[C3]), C3), "round(n0)");
        assert_eq!(emit(&Op::Power, &args(&[C3, F]), C3), "pow(n0, vec3<f32>(n1))");
        assert_eq!(emit(&Op::SafePower, &args(&[F, F]), F), "(sign(n0) * pow(abs(n0), n1))");
        assert_eq!(emit(&Op::Max, &args(&[F, F]), F), "max(n0, n1)");
    }

    #[test]
    fn clamp_is_min_of_max() {
        assert_eq!(emit(&Op::Clamp, &args(&[C3, F, F]), C3), "min(max(n0, vec3<f32>(n1)), vec3<f32>(n2))");
    }

    #[test]
    fn mix_is_bg_to_fg_with_a_scalar_amount() {
        assert_eq!(emit(&Op::Mix, &args(&[C3, C3, F]), C3), "(n1 + (n0 - n1) * n2)");
        assert_eq!(emit(&Op::Mix, &args(&[C3, F, C3]), C3), "(vec3<f32>(n1) + (n0 - vec3<f32>(n1)) * n2)");
    }

    #[test]
    fn smoothstep_is_written_out_and_guarded_at_both_ends() {
        let t = "((n0 - n1) / (n2 - n1))";
        assert_eq!(
            emit(&Op::Smoothstep, &args(&[F, F, F]), F),
            format!("select(select(({t} * {t} * (3.0 - 2.0 * {t})), 0.0, n0 <= n1), 1.0, n0 >= n2)")
        );
        let t = "((n0 - vec3<f32>(n1)) / (vec3<f32>(n2) - vec3<f32>(n1)))";
        assert_eq!(
            emit(&Op::Smoothstep, &args(&[C3, F, F]), C3),
            format!(
                "select(select(({t} * {t} * (3.0 - 2.0 * {t})), vec3<f32>(0.0), n0 <= vec3<f32>(n1)), vec3<f32>(1.0), n0 >= vec3<f32>(n2))"
            )
        );
    }

    #[test]
    fn remap() {
        assert_eq!(emit(&Op::Remap, &args(&[F, F, F, F, F]), F), "(n3 + (n0 - n1) * (n4 - n3) / (n2 - n1))");
    }
}
