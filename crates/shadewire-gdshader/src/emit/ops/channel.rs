use shadewire::{Op, Type};

use super::super::types::gd_type;
use super::{Arg, cast};

/// Mirrors `shadewire::ops::channel`.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    match op {
        Op::Extract(index) => {
            let (expr, from) = &args[0];
            assert!(
                (*index as usize) < from.component_count(),
                "extract index {index} of a {} {from:?}",
                if from.is_scalar() { "scalar" } else { "narrower" }
            );
            let component = if from.is_scalar() {
                cast(expr, *from, Type::Float)
            } else {
                format!("{expr}.{}", &"xyzw"[*index as usize..*index as usize + 1])
            };
            cast(&component, Type::Float, ty)
        }
        Op::Combine => {
            let parts: Vec<String> = args
                .iter()
                .map(|(expr, from)| if from.is_scalar() { cast(expr, *from, Type::Float) } else { expr.clone() })
                .collect();
            format!("{}({})", gd_type(ty), parts.join(", "))
        }
        Op::Convert => convert(&args[0], ty),
        other => panic!("{other:?} is not a channel op"),
    }
}

/// The ND_convert_* graphs: same width relabels, a scalar splats to every channel (alpha included),
/// wider to narrower drops trailing channels, narrower to wider appends 1.0.
fn convert(arg: &Arg, ty: Type) -> String {
    let (expr, from) = arg;
    let have = from.component_count();
    let want = ty.component_count();
    if have == 1 || have == want {
        return cast(expr, *from, ty);
    }
    if have > want {
        let narrowed = format!("{expr}.{}", &"xyzw"[..want]);
        return if want == 1 { cast(&narrowed, Type::Float, ty) } else { narrowed };
    }
    let ones = vec!["1.0"; want - have].join(", ");
    format!("{}({expr}, {ones})", gd_type(ty))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::ops::args;

    #[test]
    fn extract_combine_convert() {
        assert_eq!(emit(&Op::Extract(2), &args(&[Type::Color4]), Type::Float), "n0.z");
        assert_eq!(emit(&Op::Combine, &args(&[Type::Float, Type::Integer]), Type::Vector2), "vec2(n0, float(n1))");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Color3]), Type::Color4), "vec4(n0, 1.0)");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Vector4]), Type::Integer), "int(n0.x)");
    }
}
