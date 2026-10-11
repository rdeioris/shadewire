use crate::Space;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransformKind {
    Point,
    Vector,
    Normal,
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
