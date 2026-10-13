//! Building typed expressions for the material's nodes: literals, the interpreter's casts, operators and calls.

use std::sync::OnceLock;

use crate::shader::ast::*;
use crate::shader::infer::{Env, binary_type, builtin_type};
use crate::shader::library;
use crate::{NodeId, Value};

/// A node's value: `n{index}`.
pub fn node(id: NodeId, ty: Ty) -> Expr {
    Expr::name(format!("n{}", id.0), ty)
}

/// An expression for `value`: a boolean, an integer (`i32::MIN` by subtraction: its magnitude overflows a
/// literal), a float (non-finite ones from their bits) or a vector of floats.
///
/// # Panics
/// On matrices, which elaboration rejects.
pub fn literal(value: &Value) -> Expr {
    match value {
        Value::Boolean(b) => Expr::bool(*b),
        Value::Integer(i32::MIN) => bin(BinaryOp::Sub, Expr::i32(-i32::MAX), Expr::i32(1)),
        Value::Integer(i) => Expr::i32(*i),
        Value::Float(x) => float(*x),
        Value::Matrix33(_) | Value::Matrix44(_) => panic!("matrix types are rejected in elaboration"),
        other => Expr::construct(Ty::of(other.ty()), other.floats().into_iter().map(float).collect()),
    }
}

/// An f32 literal; NaN and the infinities have no literal, so they are their bits.
pub fn float(x: f32) -> Expr {
    if x.is_finite() { Expr::f32(x) } else { Expr::u32(x.to_bits()).bitcast(Ty::F32) }
}

/// `lhs op rhs`, typed.
pub fn bin(op: BinaryOp, lhs: Expr, rhs: Expr) -> Expr {
    let ty = binary_type(op, &lhs.ty, &rhs.ty).unwrap_or_else(|e| panic!("{e}"));
    Expr::binary(op, lhs, rhs, ty)
}

pub fn add(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Add, a, b)
}

pub fn sub(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Sub, a, b)
}

pub fn mul(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Mul, a, b)
}

pub fn div(a: Expr, b: Expr) -> Expr {
    bin(BinaryOp::Div, a, b)
}

/// A built-in call, typed.
pub fn builtin(b: Builtin, args: Vec<Expr>) -> Expr {
    let ty = builtin_type(b, &args).unwrap_or_else(|e| panic!("{e}"));
    Expr::builtin(b, args, ty)
}

/// The declarations of the support libraries, for the types of calls into them.
fn library_env() -> &'static Env {
    static ENV: OnceLock<Env> = OnceLock::new();
    ENV.get_or_init(library::env)
}

/// A call of a support library function, typed by its declaration.
pub fn lib_call(function: &str, args: Vec<Expr>) -> Expr {
    let (_, ret) = library_env().functions.get(function).unwrap_or_else(|| panic!("no library function {function}"));
    Expr::call(function, args, ret.clone())
}

/// Converts between any two types the interpreter's `broadcast` accepts, plus scalar casts: scalars convert
/// (`f32(i)`, `bool(x)`), a scalar splats to a vector (as f32), the same type is itself.
///
/// # Panics
/// On a width change between vectors: those are `Convert` ops, never casts.
pub fn cast(e: Expr, to: &Ty) -> Expr {
    if e.ty == *to {
        return e;
    }
    if e.ty.is_scalar() && to.is_scalar() {
        return e.convert(to.clone());
    }
    if e.ty.is_scalar() {
        let scalar = if e.ty == Ty::F32 { e } else { e.convert(Ty::F32) };
        return Expr::construct(to.clone(), vec![scalar]);
    }
    panic!("cast {:?} -> {to:?} is a Convert, not a cast", e.ty);
}

/// The interpreter does all arithmetic in f32 and converts the result to the node type, so integer and boolean
/// results are computed as floats.
pub fn work_type(ty: &Ty) -> Ty {
    if ty.is_scalar() { Ty::F32 } else { ty.clone() }
}

/// Every argument cast to the work type of `ty`.
pub fn wide(args: &[Expr], ty: &Ty) -> Vec<Expr> {
    let work = work_type(ty);
    args.iter().map(|a| cast(a.clone(), &work)).collect()
}

/// `Value::as_f32`: a scalar as f32, or the first component of a vector.
pub fn first_f32(arg: &Expr) -> Expr {
    if arg.ty.is_scalar() { cast(arg.clone(), &Ty::F32) } else { arg.clone().swizzle("x") }
}

/// The first `n` (2 or 3) components, like the interpreter's `floats()[..n]`: the same width is itself, a wider
/// vector drops trailing channels.
///
/// # Panics
/// On a narrower argument, as indexing does.
pub fn leading(arg: &Expr, n: u8) -> Expr {
    let have = arg.ty.width().unwrap_or(0);
    assert!(have >= n && arg.ty.is_vector(), "need {n} components, got {:?}", arg.ty);
    if have == n { arg.clone() } else { arg.clone().swizzle(&"xyzw"[..n as usize]) }
}

/// `select(f, t, cond)`.
pub fn select(f: Expr, t: Expr, cond: Expr) -> Expr {
    builtin(Builtin::Select, vec![f, t, cond])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casts_convert_splat_and_keep() {
        let i = Expr::name("i", Ty::I32);
        assert_eq!(cast(i.clone(), &Ty::F32), i.clone().convert(Ty::F32));
        let splat = cast(i.clone(), &Ty::vec(3));
        assert_eq!(splat, Expr::construct(Ty::vec(3), vec![i.convert(Ty::F32)]));
        let v = Expr::name("v", Ty::vec(3));
        assert_eq!(cast(v.clone(), &Ty::vec(3)), v);
    }

    #[test]
    fn non_finite_floats_are_bits() {
        assert_eq!(float(f32::INFINITY), Expr::u32(0x7f80_0000).bitcast(Ty::F32));
        assert_eq!(literal(&Value::Integer(i32::MIN)).ty, Ty::I32);
    }

    #[test]
    fn library_calls_take_their_declared_type() {
        assert_eq!(lib_call("shadewire_perlin_noise_vec3_2d", vec![Expr::name("p", Ty::vec(2))]).ty, Ty::vec(3));
    }
}
