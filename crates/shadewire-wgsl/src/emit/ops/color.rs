use shadewire_ir::{Op, Type};

use super::{cast, leading, Arg};

/// Mirrors `shadewire_ir::ops::color`: the first three channels go through the function,
/// color4 keeps its alpha. Luminance is written out in the interpreter's order, not `dot`.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let rgb = match op {
        Op::Luminance => {
            let (c, k) = (&args[0].0, &args[1].0);
            assert!(args[1].1.component_count() >= 3, "luminance coefficients of type {:?}", args[1].1);
            assert!(args[0].1.component_count() >= 3, "luminance of type {:?}", args[0].1);
            format!("vec3<f32>(({c}[0] * {k}[0] + {c}[1] * {k}[1] + {c}[2] * {k}[2]))")
        }
        Op::RgbToHsv => format!("mx_rgbtohsv({})", leading(&args[0], 3)),
        Op::HsvToRgb => format!("mx_hsvtorgb({})", leading(&args[0], 3)),
        other => panic!("{other:?} is not a color op"),
    };
    if ty == Type::Color4 {
        let (expr, from) = &args[0];
        assert_eq!(from.component_count(), 4, "{op:?} to color4 needs a four-channel input, got {from:?}");
        format!("vec4<f32>({rgb}, {expr}[3])")
    } else {
        cast(&rgb, Type::Color3, ty)
    }
}

#[cfg(test)]
mod tests {
    use super::emit;
    use crate::emit::test_support::args;
    use shadewire_ir::{Op, Type};

    const C3: Type = Type::Color3;
    const C4: Type = Type::Color4;

    #[test]
    fn hsv_keeps_alpha() {
        assert_eq!(emit(&Op::RgbToHsv, &args(&[C3]), C3), "mx_rgbtohsv(n0)");
        assert_eq!(emit(&Op::HsvToRgb, &args(&[C4]), C4), "vec4<f32>(mx_hsvtorgb(n0.xyz), n0[3])");
    }

    #[test]
    fn luminance_is_written_out() {
        let l = "(n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2])";
        assert_eq!(emit(&Op::Luminance, &args(&[C3, C3]), C3), format!("vec3<f32>({l})"));
        assert_eq!(emit(&Op::Luminance, &args(&[C4, C3]), C4), format!("vec4<f32>(vec3<f32>({l}), n0[3])"));
    }
}
