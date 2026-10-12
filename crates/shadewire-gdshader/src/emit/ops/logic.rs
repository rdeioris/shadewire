use shadewire::{Op, Type, Value};

use super::super::types::literal;
use super::{Arg, cast, first_f32, select};

/// Mirrors `shadewire::ops::logic`. Comparisons read `as_f32` (first component, as float);
/// equality compares every component.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let pick = |cond: String| select(&cast(&args[3].0, args[3].1, ty), &cast(&args[2].0, args[2].1, ty), &cond);
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

/// `floats() == floats()`: scalars as float, vectors componentwise, different widths never equal.
fn equal(a: &Arg, b: &Arg) -> String {
    if a.1.is_scalar() && b.1.is_scalar() {
        format!("({} == {})", first_f32(a), first_f32(b))
    } else if a.1.component_count() == b.1.component_count() {
        format!("all(equal({}, {}))", a.0, b.0)
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
        out = select(&out, &input, &format!("{which} < {k}.0"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::ops::args;

    #[test]
    fn if_greater_picks_with_a_ternary() {
        let a = args(&[Type::Float, Type::Float, Type::Color3, Type::Float]);
        assert_eq!(emit(&Op::IfGreater, &a, Type::Color3), "((n0 > n1) ? n2 : vec3(n3))");
    }

    #[test]
    fn vector_equality_is_all_equal() {
        let a = args(&[Type::Color3, Type::Vector3]);
        assert_eq!(emit(&Op::Equal, &a, Type::Boolean), "all(equal(n0, n1))");
        assert_eq!(emit(&Op::Equal, &args(&[Type::Color3, Type::Vector2]), Type::Boolean), "false");
    }

    #[test]
    fn switch_nests_from_the_last_input() {
        let mut a = args(&[Type::Float; 10]);
        a.push(("w".into(), Type::Integer));
        let text = emit(&Op::Switch, &a, Type::Float);
        assert!(text.starts_with("(float(w) < 1.0 ? n0 : (float(w) < 2.0 ? n1 : "), "{text}");
        assert!(text.ends_with("(float(w) < 10.0 ? n9 : 0.0))))))))))"), "{text}");
    }
}
