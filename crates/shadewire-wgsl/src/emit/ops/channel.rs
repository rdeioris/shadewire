use shadewire_ir::{Op, Type};

use super::super::types::wgsl_type;
use super::{Arg, cast};

/// Mirrors `shadewire_ir::ops::channel`.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    match op {
        Op::Extract(index) => {
            let (expr, from) = &args[0];
            assert!(
                (*index as usize) < from.component_count(),
                "extract index {index} of a {} {from:?}",
                if from.is_scalar() { "scalar" } else { "narrower" }
            );
            let component = if from.is_scalar() { cast(expr, *from, Type::Float) } else { format!("{expr}[{index}]") };
            cast(&component, Type::Float, ty)
        }
        Op::Combine => {
            let parts: Vec<String> = args
                .iter()
                .map(|(expr, from)| if from.is_scalar() { cast(expr, *from, Type::Float) } else { expr.clone() })
                .collect();
            format!("{}({})", wgsl_type(ty), parts.join(", "))
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
        // One component is an f32 that may still become an integer or a boolean.
        return if want == 1 { cast(&narrowed, Type::Float, ty) } else { narrowed };
    }
    let ones = vec!["1.0"; want - have].join(", ");
    format!("{}({expr}, {ones})", wgsl_type(ty))
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    #[test]
    fn extract_and_combine() {
        assert_eq!(emit(&Op::Extract(2), &args(&[Type::Color3]), Type::Float), "n0[2]");
        let a = args(&[Type::Color3, Type::Float]);
        assert_eq!(emit(&Op::Combine, &a, Type::Color4), "vec4<f32>(n0, n1)");
        let a = args(&[Type::Float, Type::Integer]);
        assert_eq!(emit(&Op::Combine, &a, Type::Vector2), "vec2<f32>(n0, f32(n1))");
    }

    #[test]
    fn convert_follows_the_interpreter() {
        assert_eq!(emit(&Op::Convert, &args(&[Type::Color3]), Type::Color4), "vec4<f32>(n0, 1.0)");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Vector2]), Type::Vector4), "vec4<f32>(n0, 1.0, 1.0)");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Float]), Type::Color4), "vec4<f32>(n0)");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Integer]), Type::Color3), "vec3<f32>(f32(n0))");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Color4]), Type::Color3), "n0.xyz");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Vector3]), Type::Vector2), "n0.xy");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Vector3]), Type::Color3), "n0");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Vector3]), Type::Float), "n0.x");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Boolean]), Type::Float), "f32(n0)");
        assert_eq!(emit(&Op::Convert, &args(&[Type::Float]), Type::Integer), "i32(n0)");
    }

    #[test]
    #[should_panic(expected = "extract index 1 of a scalar")]
    fn extract_past_a_scalar_panics() {
        emit(&Op::Extract(1), &args(&[Type::Float]), Type::Float);
    }
}
