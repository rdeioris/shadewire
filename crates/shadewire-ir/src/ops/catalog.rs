use crate::{Op, Space, TransformKind, Type};

pub fn input_names(op: &Op) -> &'static [&'static str] {
    match op {
        Op::Add
        | Op::Subtract
        | Op::Multiply
        | Op::Divide
        | Op::Modulo
        | Op::Power
        | Op::SafePower
        | Op::Min
        | Op::Max
        | Op::Distance
        | Op::DotProduct
        | Op::CrossProduct
        | Op::And
        | Op::Or => &["in1", "in2"],
        Op::Invert | Op::Rotate2d => &["in", "amount"],
        Op::Absval
        | Op::Sign
        | Op::Floor
        | Op::Ceil
        | Op::Round
        | Op::Fract
        | Op::Sqrt
        | Op::Exp
        | Op::Ln
        | Op::Sin
        | Op::Cos
        | Op::Tan
        | Op::Asin
        | Op::Acos
        | Op::Normalize
        | Op::Magnitude
        | Op::Not
        | Op::Convert
        | Op::RgbToHsv
        | Op::HsvToRgb
        | Op::Transform { .. }
        | Op::Extract(_)
        | Op::Dpdx
        | Op::Dpdy => &["in"],
        Op::Clamp | Op::Smoothstep => &["in", "low", "high"],
        Op::Mix => &["fg", "bg", "mix"],
        Op::Remap => &["in", "inlow", "inhigh", "outlow", "outhigh"],
        Op::Atan2 => &["iny", "inx"],
        Op::Greater | Op::GreaterEq | Op::Equal => &["value1", "value2"],
        Op::IfGreater | Op::IfGreaterEq | Op::IfEqual => &["value1", "value2", "in1", "in2"],
        Op::Switch => &["in1", "in2", "in3", "in4", "in5", "in6", "in7", "in8", "in9", "in10", "which"],
        Op::Combine => &["in1", "in2", "in3", "in4"],
        Op::Luminance => &["in", "lumacoeffs"],
        Op::Noise2d => &["amplitude", "pivot", "texcoord"],
        Op::Noise3d => &["amplitude", "pivot", "position"],
        Op::Fractal3d => &["amplitude", "octaves", "lacunarity", "diminish", "position"],
        Op::CellNoise2d => &["texcoord"],
        Op::CellNoise3d => &["position"],
        Op::WorleyNoise2d => &["texcoord", "jitter", "style"],
        Op::WorleyNoise3d => &["position", "jitter", "style"],
        Op::NormalMap => &["in", "scale", "normal", "tangent", "bitangent"],
        Op::HextileCoord(_) => &["coord", "rotation", "rotationrange", "scale", "scalerange", "offset", "offsetrange"],
        Op::HextileGrad(_) => &["grad", "coord", "rotation", "rotationrange", "scale", "scalerange"],
        Op::HextileWeights => &["coord", "weights", "falloff"],
    }
}

/// `input_count` is how many non-string inputs the resolved nodedef has; it splits
/// `ifgreater` (4 inputs, value result) from its boolean form (2 inputs).
pub fn op_for_category(category: &str, output: Type, input_count: usize) -> Option<Op> {
    let boolean_compare = output == Type::Boolean && input_count == 2;
    match category {
        "add" => Some(Op::Add),
        "subtract" => Some(Op::Subtract),
        "multiply" => Some(Op::Multiply),
        "divide" => Some(Op::Divide),
        "modulo" => Some(Op::Modulo),
        "invert" => Some(Op::Invert),
        "absval" => Some(Op::Absval),
        "sign" => Some(Op::Sign),
        "floor" => Some(Op::Floor),
        "ceil" => Some(Op::Ceil),
        "round" => Some(Op::Round),
        "fract" => Some(Op::Fract),
        "power" => Some(Op::Power),
        "safepower" => Some(Op::SafePower),
        "sqrt" => Some(Op::Sqrt),
        "exp" => Some(Op::Exp),
        "ln" => Some(Op::Ln),
        "min" => Some(Op::Min),
        "max" => Some(Op::Max),
        "clamp" => Some(Op::Clamp),
        "mix" => Some(Op::Mix),
        "smoothstep" => Some(Op::Smoothstep),
        "remap" => Some(Op::Remap),
        "sin" => Some(Op::Sin),
        "cos" => Some(Op::Cos),
        "tan" => Some(Op::Tan),
        "asin" => Some(Op::Asin),
        "acos" => Some(Op::Acos),
        "atan2" => Some(Op::Atan2),
        "normalize" => Some(Op::Normalize),
        "magnitude" => Some(Op::Magnitude),
        "distance" => Some(Op::Distance),
        "dotproduct" => Some(Op::DotProduct),
        "crossproduct" => Some(Op::CrossProduct),
        "rotate2d" => Some(Op::Rotate2d),
        "transformpoint" => Some(Op::Transform { kind: TransformKind::Point, from: Space::World, to: Space::World }),
        "transformvector" => Some(Op::Transform { kind: TransformKind::Vector, from: Space::World, to: Space::World }),
        "transformnormal" => Some(Op::Transform { kind: TransformKind::Normal, from: Space::World, to: Space::World }),
        "ifgreater" if boolean_compare => Some(Op::Greater),
        "ifgreatereq" if boolean_compare => Some(Op::GreaterEq),
        "ifequal" if boolean_compare => Some(Op::Equal),
        "ifgreater" => Some(Op::IfGreater),
        "ifgreatereq" => Some(Op::IfGreaterEq),
        "ifequal" => Some(Op::IfEqual),
        "switch" => Some(Op::Switch),
        "and" => Some(Op::And),
        "or" => Some(Op::Or),
        "not" => Some(Op::Not),
        "extract" => Some(Op::Extract(0)),
        "combine2" | "combine3" | "combine4" => Some(Op::Combine),
        "convert" => Some(Op::Convert),
        "luminance" => Some(Op::Luminance),
        "rgbtohsv" => Some(Op::RgbToHsv),
        "hsvtorgb" => Some(Op::HsvToRgb),
        "noise2d" => Some(Op::Noise2d),
        "noise3d" => Some(Op::Noise3d),
        "fractal3d" => Some(Op::Fractal3d),
        "cellnoise2d" => Some(Op::CellNoise2d),
        "cellnoise3d" => Some(Op::CellNoise3d),
        "worleynoise2d" => Some(Op::WorleyNoise2d),
        "worleynoise3d" => Some(Op::WorleyNoise3d),
        "normalmap" => Some(Op::NormalMap),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Op, Type};

    #[test]
    fn maps_categories() {
        assert_eq!(op_for_category("add", Type::Color3, 2), Some(Op::Add));
        assert_eq!(op_for_category("ifgreater", Type::Boolean, 2), Some(Op::Greater));
        assert_eq!(op_for_category("ifgreater", Type::Float, 4), Some(Op::IfGreater));
        assert_eq!(op_for_category("combine3", Type::Color3, 3), Some(Op::Combine));
        assert_eq!(op_for_category("standard_surface", Type::Float, 0), None);
    }

    #[test]
    fn mix_inputs_are_fg_bg_mix() {
        assert_eq!(input_names(&Op::Mix), &["fg", "bg", "mix"]);
    }

    #[test]
    fn worley_inputs_include_style() {
        assert_eq!(input_names(&Op::WorleyNoise2d), &["texcoord", "jitter", "style"]);
        assert_eq!(input_names(&Op::WorleyNoise3d), &["position", "jitter", "style"]);
    }
}
