use shadewire::{Op, Type};

use super::{Arg, cast, leading};

/// Mirrors `shadewire::ops::color`: the first three channels go through the function,
/// color4 keeps its alpha. Luminance is written out in the interpreter's order, not `dot`.
pub fn emit(op: &Op, args: &[Arg], ty: Type) -> String {
    let rgb = match op {
        Op::Luminance => {
            let (c, k) = (&args[0].0, &args[1].0);
            assert!(args[1].1.component_count() >= 3, "luminance coefficients of type {:?}", args[1].1);
            assert!(args[0].1.component_count() >= 3, "luminance of type {:?}", args[0].1);
            format!("vec3(({c}.x * {k}.x + {c}.y * {k}.y + {c}.z * {k}.z))")
        }
        Op::RgbToHsv => format!("shadewire_rgbtohsv({})", leading(&args[0], 3)),
        Op::HsvToRgb => format!("shadewire_hsvtorgb({})", leading(&args[0], 3)),
        other => panic!("{other:?} is not a color op"),
    };
    if ty == Type::Color4 {
        let (expr, from) = &args[0];
        assert_eq!(from.component_count(), 4, "{op:?} to color4 needs a four-channel input, got {from:?}");
        format!("vec4({rgb}, {expr}.w)")
    } else {
        cast(&rgb, Type::Color3, ty)
    }
}
