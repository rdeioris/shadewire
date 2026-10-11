use shadewire_ir::{Op, Type, Value};

use super::super::types::literal;
use super::{cast, first_f32, Arg};

/// Mirrors `shadewire_ir::ops::logic`. Comparisons read `as_f32` (first component, as f32);
/// equality compares every component.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let pick = |cond: String| format!("select({}, {}, {cond})", cast(&args[3].0, args[3].1, ty), cast(&args[2].0, args[2].1, ty));
    let boolean = match op {
        Op::Greater => greater(args, ">"),
        Op::GreaterEq => greater(args, ">="),
        Op::Equal => equal(&args[0], &args[1]),
        Op::IfGreater => return pick(greater(args, ">")),
        Op::IfGreaterEq => return pick(greater(args, ">=")),
        Op::IfEqual => return pick(equal(&args[0], &args[1])),
        Op::Switch => return switch(args, ty),
        Op::And => format!("({} && {})", truth(&args[0]), truth(&args[1])),
        Op::Or => format!("({} || {})", truth(&args[0]), truth(&args[1])),
        Op::Not => format!("(!{})", truth(&args[0])),
        other => panic!("{other:?} is not a logic op"),
    };
    cast(&boolean, Type::Boolean, ty)
}

fn greater(args: &[Arg], operator: &str) -> String {
    format!("({} {operator} {})", first_f32(&args[0]), first_f32(&args[1]))
}

/// `floats() == floats()`: scalars as f32, vectors componentwise, different widths never equal.
fn equal(a: &Arg, b: &Arg) -> String {
    if a.1.is_scalar() && b.1.is_scalar() {
        format!("({} == {})", first_f32(a), first_f32(b))
    } else if a.1.component_count() == b.1.component_count() {
        format!("all({} == {})", a.0, b.0)
    } else {
        "false".to_string()
    }
}

/// `Value::as_bool`.
fn truth(arg: &Arg) -> String {
    match arg.1 {
        Type::Boolean => arg.0.clone(),
        _ => format!("({} != 0.0)", first_f32(arg)),
    }
}

/// NG_switch: in_k for the first k in 1..=10 with `which < k`, else zero (NaN included).
fn switch(args: &[Arg], ty: Type) -> String {
    let which = first_f32(&args[10]);
    let mut out = literal(&Value::zero(ty));
    for k in (1..=10).rev() {
        let input = cast(&args[k - 1].0, args[k - 1].1, ty);
        out = format!("select({out}, {input}, {which} < {k}.0)");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    const F: Type = Type::Float;
    const B: Type = Type::Boolean;

    #[test]
    fn comparisons() {
        assert_eq!(emit(&Op::Greater, &args(&[F, F]), B), "(n0 > n1)");
        assert_eq!(emit(&Op::GreaterEq, &args(&[Type::Integer, Type::Integer]), B), "(f32(n0) >= f32(n1))");
        assert_eq!(emit(&Op::Equal, &args(&[F, F]), B), "(n0 == n1)");
        assert_eq!(emit(&Op::Equal, &args(&[Type::Color3, Type::Color3]), B), "all(n0 == n1)");
        assert_eq!(emit(&Op::Equal, &args(&[B, B]), B), "(f32(n0) == f32(n1))");
    }

    #[test]
    fn if_ops_select() {
        let a = args(&[F, F, Type::Color3, F]);
        assert_eq!(emit(&Op::IfGreater, &a, Type::Color3), "select(vec3<f32>(n3), n2, (n0 > n1))");
        assert_eq!(emit(&Op::IfGreaterEq, &args(&[F, F, F, F]), F), "select(n3, n2, (n0 >= n1))");
        assert_eq!(emit(&Op::IfEqual, &args(&[Type::Integer, Type::Integer, F, F]), F), "select(n3, n2, (f32(n0) == f32(n1)))");
    }

    #[test]
    fn switch_is_the_materialx_chain() {
        let a = args(&[F; 11]);
        let expected = "select(select(select(select(select(select(select(select(select(select(0.0, n9, n10 < 10.0), \
            n8, n10 < 9.0), n7, n10 < 8.0), n6, n10 < 7.0), n5, n10 < 6.0), n4, n10 < 5.0), n3, n10 < 4.0), \
            n2, n10 < 3.0), n1, n10 < 2.0), n0, n10 < 1.0)";
        assert_eq!(emit(&Op::Switch, &a, F), expected);
        let ints = args(&[Type::Integer; 11]);
        let text = emit(&Op::Switch, &ints, Type::Integer);
        let head = "select(select(select(select(select(select(select(select(select(select(0i, n9, f32(n10) < 10.0)";
        assert!(text.starts_with(head), "{text}");
    }

    #[test]
    fn booleans() {
        assert_eq!(emit(&Op::And, &args(&[B, B]), B), "(n0 && n1)");
        assert_eq!(emit(&Op::Or, &args(&[B, B]), B), "(n0 || n1)");
        assert_eq!(emit(&Op::Not, &args(&[B]), B), "(!n0)");
        assert_eq!(emit(&Op::Not, &args(&[F]), B), "(!(n0 != 0.0))");
    }
}
