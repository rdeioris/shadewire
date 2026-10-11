//! One WGSL expression per IR op. Each family file mirrors `shadewire_ir::ops::<family>`
//! and must compute what the interpreter computes.

mod channel;
mod logic;
mod math;
mod space;
mod trig;
mod vector;

use shadewire_ir::{Op, Type};

use super::coerce::coerce;
use super::types::wgsl_type;

/// An argument: its WGSL expression and its IR type.
pub type Arg = (String, Type);

pub fn emit_op(op: &Op, args: &[Arg], ty: Type) -> String {
    match op {
        Op::Add | Op::Subtract | Op::Multiply | Op::Divide | Op::Modulo | Op::Invert | Op::Absval | Op::Sign
        | Op::Floor | Op::Ceil | Op::Round | Op::Fract | Op::Power | Op::SafePower | Op::Sqrt | Op::Exp | Op::Ln
        | Op::Min | Op::Max | Op::Clamp | Op::Mix | Op::Smoothstep | Op::Remap => math::emit(op, args, ty),
        Op::Sin | Op::Cos | Op::Tan | Op::Asin | Op::Acos | Op::Atan2 => trig::emit(op, args, ty),
        Op::Normalize | Op::Magnitude | Op::Distance | Op::DotProduct | Op::CrossProduct => vector::emit(op, args, ty),
        Op::Transform { .. } => space::emit(op, args, ty),
        Op::Greater | Op::GreaterEq | Op::Equal | Op::IfGreater | Op::IfGreaterEq | Op::IfEqual | Op::Switch
        | Op::And | Op::Or | Op::Not => logic::emit(op, args, ty),
        Op::Extract(_) | Op::Combine | Op::Convert => channel::emit(op, args, ty),
        Op::Luminance | Op::RgbToHsv | Op::HsvToRgb => unimplemented!("task 21: color ops"),
        Op::Noise2d | Op::Noise3d | Op::Fractal3d | Op::CellNoise2d | Op::CellNoise3d
        | Op::WorleyNoise2d | Op::WorleyNoise3d => unimplemented!("task 21: noise ops"),
        Op::NormalMap => unimplemented!("task 21: normalmap"),
    }
}

/// Converts between any two types the interpreter's `broadcast` accepts, plus scalar casts:
/// scalars cast with `i32(..)`/`f32(..)`/`bool(..)`, splat to vectors, same widths relabel.
pub fn cast(expr: &str, from: Type, to: Type) -> String {
    if from == to {
        return expr.to_string();
    }
    if from.is_scalar() && to.is_scalar() {
        return format!("{}({expr})", wgsl_type(to));
    }
    coerce(expr, from, to)
}

/// The interpreter does all arithmetic in f32 and converts the result to the node type,
/// so integer and boolean results are computed as floats.
pub fn work_type(ty: Type) -> Type {
    if ty.is_scalar() {
        Type::Float
    } else {
        ty
    }
}

/// Every argument cast to the work type of `ty`.
pub fn wide(args: &[Arg], ty: Type) -> Vec<String> {
    let work = work_type(ty);
    args.iter().map(|(expr, from)| cast(expr, *from, work)).collect()
}

/// Converts a work-type result back to the node type.
pub fn finish(expr: String, ty: Type) -> String {
    cast(&expr, work_type(ty), ty)
}

/// `Value::as_f32`: a scalar as f32, or the first component of a vector.
pub fn first_f32(arg: &Arg) -> String {
    let (expr, ty) = arg;
    if ty.is_scalar() {
        cast(expr, *ty, Type::Float)
    } else {
        format!("{expr}.x")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::args;

    #[test]
    fn casts() {
        assert_eq!(cast("n0", Type::Integer, Type::Float), "f32(n0)");
        assert_eq!(cast("n0", Type::Float, Type::Integer), "i32(n0)");
        assert_eq!(cast("n0", Type::Float, Type::Boolean), "bool(n0)");
        assert_eq!(cast("n0", Type::Float, Type::Color3), "vec3<f32>(n0)");
        assert_eq!(cast("n0", Type::Vector3, Type::Color3), "n0");
    }

    #[test]
    fn integer_math_runs_in_f32() {
        assert_eq!(emit_op(&Op::Add, &args(&[Type::Integer, Type::Integer]), Type::Integer), "i32((f32(n0) + f32(n1)))");
    }

    #[test]
    fn first_component() {
        assert_eq!(first_f32(&args(&[Type::Vector2])[0]), "n0.x");
        assert_eq!(first_f32(&args(&[Type::Boolean])[0]), "f32(n0)");
    }
}
