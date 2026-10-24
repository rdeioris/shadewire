//! Arithmetic, trigonometry, interpolation and comparisons, each with Unreal's constants for unlinked inputs and
//! its rules for operand widths.

use std::f32::consts::TAU;

use shadewire::{Error, NodeId, Type};
use shadewire_nodes::Arg;

use super::Lowerer;
use crate::document::Expression;

/// `A op B`, each falling back to `ConstA` and `ConstB` (Unreal's Add, Subtract, Multiply, Divide, Min, Max).
pub fn binary(l: &mut Lowerer, f: usize, x: &Expression, category: &str) -> Result<NodeId, Error> {
    let a = l.input_or(f, x, "A", "ConstA", 0.0)?;
    let b = l.input_or(f, x, "B", "ConstB", 1.0)?;
    l.op2(category, a, b, &l.origin(f, x))
}

/// A one-input node over `Input`.
pub fn unary(l: &mut Lowerer, f: usize, x: &Expression, category: &str) -> Result<NodeId, Error> {
    let input = first(l, f, x)?;
    l.op1(category, input, &l.origin(f, x))
}

/// The expression's first input, whatever Unreal names it (`Input`, `VectorInput`, `None`).
pub fn first(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    match l.input_at(f, x, 0)? {
        Some(id) => Ok(id),
        None => {
            l.warn(f, x, "nothing is linked, read as 0");
            Ok(l.k(0.0, &l.origin(f, x)))
        }
    }
}

pub fn one_minus(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    let one = l.k(1.0, &o);
    l.op2("subtract", one, input, &o)
}

pub fn saturate(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    clamp01(l, input, &o)
}

/// `id` limited to zero and one.
pub fn clamp01(l: &mut Lowerer, id: NodeId, origin: &str) -> Result<NodeId, Error> {
    let (zero, one) = (l.k(0.0, origin), l.k(1.0, origin));
    let low = l.op2("max", id, zero, origin)?;
    l.op2("min", low, one, origin)
}

/// Clamp with `MinDefault` and `MaxDefault`, and `ClampMode` saying which bounds apply.
pub fn clamp(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = l.required(f, x, "Input")?;
    let mode = x.text("ClampMode").unwrap_or("CMODE_Clamp").to_string();
    let mut out = input;
    if mode != "CMODE_ClampMax" {
        let low = l.input_or(f, x, "Min", "MinDefault", 0.0)?;
        out = l.op2("max", out, low, &o)?;
    }
    if mode != "CMODE_ClampMin" {
        let high = l.input_or(f, x, "Max", "MaxDefault", 1.0)?;
        out = l.op2("min", out, high, &o)?;
    }
    Ok(out)
}

/// `Base ^ Exponent`, the base clamped at zero as Unreal's `PositiveClampedPow` does.
pub fn power(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let base = l.required(f, x, "Base")?;
    let exponent = l.input_or(f, x, "Exponent", "ConstExponent", 2.0)?;
    let zero = l.k(0.0, &o);
    let base = l.op2("max", base, zero, &o)?;
    l.op2("power", base, exponent, &o)
}

/// HLSL's `fmod`, which truncates: `a - b * trunc(a / b)`.
pub fn fmod(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.required(f, x, "A")?;
    let b = l.required(f, x, "B")?;
    let q = l.op2("divide", a, b, &o)?;
    let t = trunc(l, q, &o)?;
    let bt = l.op2("multiply", b, t, &o)?;
    l.op2("subtract", a, bt, &o)
}

pub fn truncate(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    trunc(l, input, &o)
}

/// `sign(x) * floor(abs(x))`.
fn trunc(l: &mut Lowerer, id: NodeId, origin: &str) -> Result<NodeId, Error> {
    let abs = l.op1("absval", id, origin)?;
    let floor = l.op1("floor", abs, origin)?;
    let sign = l.op1("sign", id, origin)?;
    l.op2("multiply", sign, floor, origin)
}

/// `lerp(A, B, Alpha)`, with `ConstA`, `ConstB` and `ConstAlpha`.
pub fn lerp(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.input_or(f, x, "A", "ConstA", 0.0)?;
    let b = l.input_or(f, x, "B", "ConstB", 1.0)?;
    let alpha = l.input_or(f, x, "Alpha", "ConstAlpha", 0.5)?;
    let (a, b) = l.widen(a, b, &o)?;
    let n = l.dims(a);
    let alpha = if l.dims(alpha) == 1 { alpha } else { l.cast(alpha, n, &o)? };
    let ty = l.e.dag.node(a).ty;
    l.op("mix", ty, &[("fg", Arg::Node(b)), ("bg", Arg::Node(a)), ("mix", Arg::Node(alpha))], &o)
}

/// Sine, Cosine and Tangent take a `Period`: `sin(2π x / Period)`, the plain function when it is zero.
pub fn periodic(l: &mut Lowerer, f: usize, x: &Expression, category: &str) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    let period = x.number_or("Period", 1.0);
    let angle = if period > 0.0 {
        let scale = l.k(TAU / period, &o);
        l.op2("multiply", input, scale, &o)?
    } else {
        input
    };
    l.op1(category, angle, &o)
}

pub fn atan2(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let y = l.required(f, x, "Y")?;
    let xx = l.required(f, x, "X")?;
    let (y, xx) = l.widen(y, xx, &o)?;
    let ty = l.e.dag.node(y).ty;
    l.op("atan2", ty, &[("iny", Arg::Node(y)), ("inx", Arg::Node(xx))], &o)
}

/// `If`: `A >= B ? Greater : Less`, and the `A == B` branch within `EqualsThreshold` when it is linked.
pub fn if_compare(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.required(f, x, "A")?;
    let b = l.input_or(f, x, "B", "ConstB", 0.0)?;
    let greater = l.required(f, x, "AGreaterThanB")?;
    let less = l.required(f, x, "ALessThanB")?;
    let a = l.cast(a, 1, &o)?;
    let b = l.cast(b, 1, &o)?;
    let (greater, less) = l.widen(greater, less, &o)?;
    let ty = l.e.dag.node(greater).ty;
    let select = |l: &mut Lowerer, cat: &str, v1, v2, in1, in2| {
        let inputs =
            [("value1", Arg::Node(v1)), ("value2", Arg::Node(v2)), ("in1", Arg::Node(in1)), ("in2", Arg::Node(in2))];
        l.op(cat, ty, &inputs, &o)
    };
    let ordered = select(l, "ifgreatereq", a, b, greater, less)?;
    let Some(equal) = l.input(f, x, "AEqualsB")? else { return Ok(ordered) };
    let n = l.dims(greater);
    let equal = l.cast(equal, n, &o)?;
    let diff = l.op2("subtract", a, b, &o)?;
    let distance = l.op1("absval", diff, &o)?;
    let threshold = l.k(x.number_or("EqualsThreshold", 1e-5), &o);
    select(l, "ifgreater", distance, threshold, ordered, equal)
}

/// `Step`: `X >= Y ? 1 : 0`.
pub fn step(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let y = l.input_or(f, x, "Y", "ConstY", 0.0)?;
    let xx = l.input_or(f, x, "X", "ConstX", 0.0)?;
    let (xx, y) = l.widen(xx, y, &o)?;
    let (one, zero) = (l.k(1.0, &o), l.k(0.0, &o));
    let mut channels = Vec::new();
    for i in 0..l.dims(xx) as u32 {
        let (xi, yi) = (l.channel(xx, i, &o)?, l.channel(y, i, &o)?);
        let inputs =
            [("value1", Arg::Node(xi)), ("value2", Arg::Node(yi)), ("in1", Arg::Node(one)), ("in2", Arg::Node(zero))];
        channels.push(l.op("ifgreatereq", Type::Float, &inputs, &o)?);
    }
    l.combine(&channels, &o)
}

/// `SmoothStep(Min, Max, Value)`.
pub fn smooth_step(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let low = l.input_or(f, x, "Min", "ConstMin", 0.0)?;
    let high = l.input_or(f, x, "Max", "ConstMax", 1.0)?;
    let value = l.input_or(f, x, "Value", "ConstValue", 0.0)?;
    let (value, low) = l.widen(value, low, &o)?;
    let (value, high) = l.widen(value, high, &o)?;
    let ty = l.e.dag.node(value).ty;
    l.op("smoothstep", ty, &[("in", Arg::Node(value)), ("low", Arg::Node(low)), ("high", Arg::Node(high))], &o)
}
