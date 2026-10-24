//! Constants and the vector expressions: appending, masking, products, lengths and desaturation.

use shadewire::{Error, NodeId, Type, Value};
use shadewire_nodes::Arg;

use super::math::first;
use super::{Lowerer, short};
use crate::document::Expression;

/// Constant, Constant2Vector, Constant3Vector and Constant4Vector, as plain constants (Unreal does not expose
/// them for editing).
pub fn constant(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let values: Vec<f32> = match short(&x.class) {
        "Constant" => vec![x.number_or("R", 0.0)],
        "Constant2Vector" => vec![x.number_or("R", 0.0), x.number_or("G", 0.0)],
        "Constant3Vector" => x.numbers("Constant").unwrap_or_default().into_iter().chain([0.0; 3]).take(3).collect(),
        _ => x.numbers("Constant").unwrap_or_default().into_iter().chain([0.0; 4]).take(4).collect(),
    };
    Ok(l.kv(&values, &o))
}

/// `A` followed by `B`'s components.
pub fn append(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.required(f, x, "A")?;
    let b = l.required(f, x, "B")?;
    let mut channels = Vec::new();
    for id in [a, b] {
        let id = l.plain(id, &o)?;
        for i in 0..l.dims(id) as u32 {
            channels.push(l.channel(id, i, &o)?);
        }
    }
    if channels.len() > 4 {
        l.warn(f, x, format!("{} components appended, the first 4 kept", channels.len()));
        channels.truncate(4);
    }
    l.combine(&channels, &o)
}

pub fn component_mask(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    let input = l.plain(input, &o)?;
    let mask = ["R", "G", "B", "A"].map(|c| x.flag(c) as u8);
    if mask == [0; 4] {
        l.warn(f, x, "no component selected, read as 0");
        return Ok(l.k(0.0, &o));
    }
    l.mask(input, mask, &o)
}

pub fn dot(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.required(f, x, "A")?;
    let b = l.required(f, x, "B")?;
    let (a, b) = l.widen(a, b, &o)?;
    if l.dims(a) == 1 {
        return l.op2("multiply", a, b, &o);
    }
    l.op("dotproduct", Type::Float, &[("in1", Arg::Node(a)), ("in2", Arg::Node(b))], &o)
}

pub fn cross(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.required(f, x, "A")?;
    let b = l.required(f, x, "B")?;
    let (a, b) = (l.cast(a, 3, &o)?, l.cast(b, 3, &o)?);
    l.op("crossproduct", Type::Vector3, &[("in1", Arg::Node(a)), ("in2", Arg::Node(b))], &o)
}

pub fn normalize(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    let input = l.plain(input, &o)?;
    if l.dims(input) == 1 {
        return l.op1("sign", input, &o);
    }
    l.op1("normalize", input, &o)
}

pub fn length(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = first(l, f, x)?;
    let input = l.plain(input, &o)?;
    if l.dims(input) == 1 {
        return l.op1("absval", input, &o);
    }
    l.op("magnitude", Type::Float, &[("in", Arg::Node(input))], &o)
}

pub fn distance(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let a = l.required(f, x, "A")?;
    let b = l.required(f, x, "B")?;
    let (a, b) = l.widen(a, b, &o)?;
    let d = l.op2("subtract", a, b, &o)?;
    if l.dims(d) == 1 {
        return l.op1("absval", d, &o);
    }
    l.op("magnitude", Type::Float, &[("in", Arg::Node(d))], &o)
}

/// `lerp(color, dot(color, LuminanceFactors), Fraction)`, over the input's first three components.
pub fn desaturation(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let input = l.required(f, x, "Input")?;
    let color = l.cast(input, 3, &o)?;
    let factors = x.numbers("LuminanceFactors").unwrap_or_else(|| vec![0.3, 0.59, 0.11]);
    let factors = l.e.constant(Value::Vector3([factors[0], factors[1], factors[2]]), &o);
    let grey = l.op("dotproduct", Type::Float, &[("in1", Arg::Node(color)), ("in2", Arg::Node(factors))], &o)?;
    let grey = l.cast(grey, 3, &o)?;
    let fraction = match l.input(f, x, "Fraction")? {
        Some(id) => l.cast(id, 1, &o)?,
        None => l.k(1.0, &o),
    };
    l.op("mix", Type::Vector3, &[("fg", Arg::Node(grey)), ("bg", Arg::Node(color)), ("mix", Arg::Node(fraction))], &o)
}
