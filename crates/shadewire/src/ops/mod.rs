pub mod broadcast;
pub mod catalog;
pub mod channel;
pub mod color;
pub mod derivative;
pub mod hextile;
pub mod logic;
pub mod math;
pub mod noise;
pub mod normal;
pub mod space;
pub mod trig;
pub mod vector;

pub use catalog::{input_names, op_for_category};

use crate::interpret::ContextSource;
use crate::{Op, Type, Value};

pub fn eval_op(op: &Op, args: &[Value], ty: Type, ctx: &mut dyn ContextSource) -> Value {
    match op {
        Op::Add
        | Op::Subtract
        | Op::Multiply
        | Op::Divide
        | Op::Modulo
        | Op::Invert
        | Op::Absval
        | Op::Sign
        | Op::Floor
        | Op::Ceil
        | Op::Round
        | Op::Fract
        | Op::Power
        | Op::SafePower
        | Op::Sqrt
        | Op::Exp
        | Op::Ln
        | Op::Min
        | Op::Max
        | Op::Clamp
        | Op::Mix
        | Op::Smoothstep
        | Op::Remap => math::eval(op, args, ty),
        Op::Sin | Op::Cos | Op::Tan | Op::Asin | Op::Acos | Op::Atan2 => trig::eval(op, args, ty),
        Op::Normalize | Op::Magnitude | Op::Distance | Op::DotProduct | Op::CrossProduct | Op::Rotate2d => {
            vector::eval(op, args, ty)
        }
        Op::Transform { .. } => space::eval(op, args, ty, ctx),
        Op::Greater
        | Op::GreaterEq
        | Op::Equal
        | Op::IfGreater
        | Op::IfGreaterEq
        | Op::IfEqual
        | Op::Switch
        | Op::And
        | Op::Or
        | Op::Not => logic::eval(op, args, ty),
        Op::Extract(_) | Op::Combine | Op::Convert => channel::eval(op, args, ty),
        Op::Luminance | Op::RgbToHsv | Op::HsvToRgb => color::eval(op, args, ty),
        Op::Noise2d
        | Op::Noise3d
        | Op::Fractal2d
        | Op::Fractal3d
        | Op::CellNoise2d
        | Op::CellNoise3d
        | Op::WorleyNoise2d
        | Op::WorleyNoise3d => noise::eval(op, args, ty),
        Op::NormalMap => normal::eval(op, args, ty),
        Op::Dpdx | Op::Dpdy => derivative::eval(op, args, ty),
        Op::HextileCoord(_) | Op::HextileGrad(_) | Op::HextileWeights => hextile::eval(op, args, ty),
    }
}
