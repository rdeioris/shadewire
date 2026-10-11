use crate::Space;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransformKind {
    Point,
    Vector,
    Normal,
}

impl TransformKind {
    pub fn name(self) -> &'static str {
        match self {
            TransformKind::Point => "point",
            TransformKind::Vector => "vector",
            TransformKind::Normal => "normal",
        }
    }
}

/// `transform_<kind>_<from>_to_<to>`: the name reflection lists; the WGSL host function is `mx_` + this.
pub fn transform_name(kind: TransformKind, from: Space, to: Space) -> String {
    format!("transform_{}_{}_to_{}", kind.name(), from.name(), to.name())
}

/// Every leaf operation phase 1 implements. Componentwise unless noted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    // math.rs
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Invert,
    Absval,
    Sign,
    Floor,
    Ceil,
    Round,
    Fract,
    Power,
    SafePower,
    Sqrt,
    Exp,
    Ln,
    Min,
    Max,
    Clamp,
    Mix,
    Smoothstep,
    Remap,
    // trig.rs
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan2,
    // vector.rs
    Normalize,
    Magnitude,
    Distance,
    DotProduct,
    CrossProduct,
    Rotate2d,
    // space.rs
    Transform { kind: TransformKind, from: Space, to: Space },
    // logic.rs
    Greater,
    GreaterEq,
    Equal,
    IfGreater,
    IfGreaterEq,
    IfEqual,
    Switch,
    And,
    Or,
    Not,
    // channel.rs
    Extract(u32),
    Combine,
    Convert,
    // color.rs
    Luminance,
    RgbToHsv,
    HsvToRgb,
    // noise/
    Noise2d,
    Noise3d,
    Fractal3d,
    CellNoise2d,
    CellNoise3d,
    WorleyNoise2d,
    WorleyNoise3d,
    // normal.rs
    NormalMap,
}

impl Op {
    pub fn reads_context(&self) -> bool {
        matches!(self, Op::Transform { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_names() {
        assert_eq!(transform_name(TransformKind::Normal, Space::Object, Space::World), "transform_normal_object_to_world");
        assert_eq!(transform_name(TransformKind::Point, Space::Model, Space::World), "transform_point_model_to_world");
    }
}
