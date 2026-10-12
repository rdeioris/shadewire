//! One Godot shading language expression per IR op. Each family file mirrors
//! `shadewire::ops::<family>` and the WGSL back end's emitter of the same name,
//! and must compute what the interpreter computes.

mod channel;
mod color;
mod derivative;
mod hextile;
mod logic;
mod math;
mod noise;
mod normal;
mod space;
mod trig;
mod vector;

use shadewire::{Op, Type};

use super::types::gd_type;

pub use space::needs_matrices;

/// An argument: its expression and its IR type.
pub type Arg = (String, Type);

pub fn emit_op(op: &Op, args: &[Arg], ty: Type) -> String {
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
        | Op::Remap => math::emit(op, args, ty),
        Op::Sin | Op::Cos | Op::Tan | Op::Asin | Op::Acos | Op::Atan2 => trig::emit(op, args, ty),
        Op::Normalize | Op::Magnitude | Op::Distance | Op::DotProduct | Op::CrossProduct | Op::Rotate2d => {
            vector::emit(op, args, ty)
        }
        Op::Transform { .. } => space::emit(op, args, ty),
        Op::Greater
        | Op::GreaterEq
        | Op::Equal
        | Op::IfGreater
        | Op::IfGreaterEq
        | Op::IfEqual
        | Op::Switch
        | Op::And
        | Op::Or
        | Op::Not => logic::emit(op, args, ty),
        Op::Extract(_) | Op::Combine | Op::Convert => channel::emit(op, args, ty),
        Op::Luminance | Op::RgbToHsv | Op::HsvToRgb => color::emit(op, args, ty),
        Op::Noise2d
        | Op::Noise3d
        | Op::Fractal3d
        | Op::CellNoise2d
        | Op::CellNoise3d
        | Op::WorleyNoise2d
        | Op::WorleyNoise3d => noise::emit(op, args, ty),
        Op::NormalMap => normal::emit(op, args, ty),
        Op::Dpdx | Op::Dpdy => derivative::emit(op, args, ty),
        Op::HextileCoord(_) | Op::HextileGrad(_) | Op::HextileWeights => hextile::emit(op, args, ty),
    }
}

/// Converts between any two types the interpreter's `broadcast` accepts, plus scalar casts:
/// scalars cast with `int(..)`/`float(..)`/`bool(..)`, splat to vectors, same widths relabel.
/// 3 <-> 4 component changes are `Convert` ops, never casts.
pub fn cast(expr: &str, from: Type, to: Type) -> String {
    if from == to {
        return expr.to_string();
    }
    if from.is_scalar() && to.is_scalar() {
        return format!("{}({expr})", gd_type(to));
    }
    if from.is_scalar() {
        let scalar = match from {
            Type::Float => expr.to_string(),
            _ => format!("float({expr})"),
        };
        return format!("{}({scalar})", gd_type(to));
    }
    if from.component_count() == to.component_count() {
        return expr.to_string();
    }
    panic!("cast {from:?} -> {to:?} is a Convert, not a cast");
}

/// `cond ? t : f`; `cond` is a scalar boolean expression. Godot's ternary evaluates one side
/// only, so neither side may hold an implicit-derivative texture lookup.
pub fn select(f: &str, t: &str, cond: &str) -> String {
    format!("({cond} ? {t} : {f})")
}

/// The interpreter does all arithmetic in f32 and converts the result to the node type,
/// so integer and boolean results are computed as floats.
pub fn work_type(ty: Type) -> Type {
    if ty.is_scalar() { Type::Float } else { ty }
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

/// `Value::as_f32`: a scalar as float, or the first component of a vector.
pub fn first_f32(arg: &Arg) -> String {
    let (expr, ty) = arg;
    if ty.is_scalar() { cast(expr, *ty, Type::Float) } else { format!("{expr}.x") }
}

/// The first `n` (2 or 3) components as a `vecN`, like the interpreter's `floats()[..n]`:
/// the same width relabels, a wider vector drops trailing channels. Narrower panics, as indexing does.
pub fn leading(arg: &Arg, n: usize) -> String {
    let (expr, from) = arg;
    let have = from.component_count();
    assert!(have >= n && !from.is_scalar(), "need {n} components, got {from:?}");
    if have == n { expr.clone() } else { format!("{expr}.{}", &"xyzw"[..n]) }
}

#[cfg(test)]
pub(crate) fn args(types: &[Type]) -> Vec<Arg> {
    types.iter().enumerate().map(|(i, t)| (format!("n{i}"), *t)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casts() {
        assert_eq!(cast("n0", Type::Integer, Type::Float), "float(n0)");
        assert_eq!(cast("n0", Type::Float, Type::Integer), "int(n0)");
        assert_eq!(cast("n0", Type::Float, Type::Boolean), "bool(n0)");
        assert_eq!(cast("n0", Type::Float, Type::Color3), "vec3(n0)");
        assert_eq!(cast("n0", Type::Integer, Type::Vector2), "vec2(float(n0))");
        assert_eq!(cast("n0", Type::Vector3, Type::Color3), "n0");
    }

    #[test]
    #[should_panic(expected = "Convert")]
    fn width_changes_are_not_casts() {
        cast("x", Type::Color3, Type::Color4);
    }

    #[test]
    fn integer_math_runs_in_float() {
        assert_eq!(
            emit_op(&Op::Add, &args(&[Type::Integer, Type::Integer]), Type::Integer),
            "int((float(n0) + float(n1)))"
        );
    }

    #[test]
    fn leading_and_first_components() {
        assert_eq!(leading(&args(&[Type::Color3])[0], 3), "n0");
        assert_eq!(leading(&args(&[Type::Vector4])[0], 2), "n0.xy");
        assert_eq!(first_f32(&args(&[Type::Vector2])[0]), "n0.x");
        assert_eq!(first_f32(&args(&[Type::Boolean])[0]), "float(n0)");
    }
}
