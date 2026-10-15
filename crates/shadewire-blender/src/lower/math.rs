//! The Math, Vector Math, Clamp and Map Range nodes as MaterialX math: Blender's formulas, written out where the
//! nodedef of the same name behaves differently (safe division, truncated modulo, round half up).

use std::f32::consts::PI;

use shadewire::{Error, NodeId, Type, Value};
use shadewire_nodes::Arg;

use super::Lowerer;
use crate::graph::FlatNode;

const F: Type = Type::Float;
const V: Type = Type::Vector3;

/// How many of the Math node's three values `op` reads; one for an operation this version does not know.
fn math_arity(op: &str) -> usize {
    match op {
        "MULTIPLY_ADD" | "COMPARE" | "SMOOTH_MIN" | "SMOOTH_MAX" | "WRAP" => 3,
        "ADD" | "SUBTRACT" | "MULTIPLY" | "DIVIDE" | "POWER" | "LOGARITHM" | "MINIMUM" | "MAXIMUM" | "LESS_THAN"
        | "GREATER_THAN" | "MODULO" | "FLOORED_MODULO" | "SNAP" | "PINGPONG" | "ARCTAN2" => 2,
        _ => 1,
    }
}

/// The Math node: `Value` is the operation over its first `math_arity` values, clamped to 0..1 by `use_clamp`.
pub fn math(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let op = node.text("operation").unwrap_or("ADD");
    let o = node.name.as_str();
    let a = l.input(node, "Value", F)?;
    // The slots an operation does not read stay `a`; no formula below touches them.
    let mut operands = [a; 3];
    for (slot, socket) in operands.iter_mut().zip(["Value", "Value_001", "Value_002"]).take(math_arity(op)).skip(1) {
        *slot = l.input(node, socket, F)?;
    }
    let result = match float_op(l, op, operands, o)? {
        Some(result) => result,
        None => {
            l.warn(node, format!("operation {op} not lowered, the first value passes through"));
            a
        }
    };
    if node.flag("use_clamp") { l.clamp01(result, F, o) } else { Ok(result) }
}

/// Blender's float operation `op` over `[a, b, c]`; `None` for an operation this version does not know.
fn float_op(l: &mut Lowerer, op: &str, [a, b, c]: [NodeId; 3], o: &str) -> Result<Option<NodeId>, Error> {
    let result = match op {
        "ADD" => l.op2("add", F, a, b, o)?,
        "SUBTRACT" => l.op2("subtract", F, a, b, o)?,
        "MULTIPLY" => l.op2("multiply", F, a, b, o)?,
        "DIVIDE" => {
            let (zero, quotient) = (l.k(0.0, o), l.op2("divide", F, a, b, o)?);
            l.if_equal(b, zero, zero, quotient, F, o)?
        }
        "MULTIPLY_ADD" => {
            let product = l.op2("multiply", F, a, b, o)?;
            l.op2("add", F, product, c, o)?
        }
        "POWER" => l.op2("safepower", F, a, b, o)?,
        "LOGARITHM" => {
            let (ln_a, ln_b) = (l.op1("ln", F, a, o)?, l.op1("ln", F, b, o)?);
            l.op2("divide", F, ln_a, ln_b, o)?
        }
        "SQRT" => {
            let zero = l.k(0.0, o);
            let positive = l.op2("max", F, a, zero, o)?;
            l.op1("sqrt", F, positive, o)?
        }
        "INVERSE_SQRT" => {
            let (zero, one) = (l.k(0.0, o), l.k(1.0, o));
            let root = l.op1("sqrt", F, a, o)?;
            let inverse = l.op2("divide", F, one, root, o)?;
            l.if_greater(a, zero, inverse, zero, F, o)?
        }
        "ABSOLUTE" => l.op1("absval", F, a, o)?,
        "EXPONENT" => l.op1("exp", F, a, o)?,
        "MINIMUM" => l.op2("min", F, a, b, o)?,
        "MAXIMUM" => l.op2("max", F, a, b, o)?,
        "SIGN" => l.op1("sign", F, a, o)?,
        "FLOOR" => l.op1("floor", F, a, o)?,
        "CEIL" => l.op1("ceil", F, a, o)?,
        "LESS_THAN" => {
            let (one, zero) = (l.k(1.0, o), l.k(0.0, o));
            l.if_greater(b, a, one, zero, F, o)?
        }
        "GREATER_THAN" => {
            let (one, zero) = (l.k(1.0, o), l.k(0.0, o));
            l.if_greater(a, b, one, zero, F, o)?
        }
        "COMPARE" => {
            let (one, zero) = (l.k(1.0, o), l.k(0.0, o));
            let difference = l.op2("subtract", F, a, b, o)?;
            let distance = l.op1("absval", F, difference, o)?;
            l.if_greater_eq(c, distance, one, zero, F, o)?
        }
        "SMOOTH_MIN" => smooth_min(l, a, b, c, o)?,
        "SMOOTH_MAX" => {
            let minus_one = l.k(-1.0, o);
            let (na, nb) = (l.op2("multiply", F, a, minus_one, o)?, l.op2("multiply", F, b, minus_one, o)?);
            let smooth = smooth_min(l, na, nb, c, o)?;
            l.op2("multiply", F, smooth, minus_one, o)?
        }
        "ROUND" => {
            let half = l.k(0.5, o);
            let shifted = l.op2("add", F, a, half, o)?;
            l.op1("floor", F, shifted, o)?
        }
        "TRUNC" => trunc(l, F, a, o)?,
        "FRACT" => l.op1("fract", F, a, o)?,
        "MODULO" => {
            let (zero, quotient) = (l.k(0.0, o), l.op2("divide", F, a, b, o)?);
            let whole = trunc(l, F, quotient, o)?;
            let multiple = l.op2("multiply", F, b, whole, o)?;
            let remainder = l.op2("subtract", F, a, multiple, o)?;
            l.if_equal(b, zero, zero, remainder, F, o)?
        }
        "FLOORED_MODULO" => {
            let zero = l.k(0.0, o);
            let remainder = l.op2("modulo", F, a, b, o)?;
            l.if_equal(b, zero, zero, remainder, F, o)?
        }
        "WRAP" => {
            let range = l.op2("subtract", F, b, c, o)?;
            let offset = l.op2("subtract", F, a, c, o)?;
            let wrapped = l.op2("modulo", F, offset, range, o)?;
            let wrapped = l.op2("add", F, c, wrapped, o)?;
            let zero = l.k(0.0, o);
            l.if_equal(range, zero, c, wrapped, F, o)?
        }
        "SNAP" => {
            let (zero, quotient) = (l.k(0.0, o), l.op2("divide", F, a, b, o)?);
            let steps = l.op1("floor", F, quotient, o)?;
            let snapped = l.op2("multiply", F, steps, b, o)?;
            l.if_equal(b, zero, zero, snapped, F, o)?
        }
        "PINGPONG" => {
            let (zero, two) = (l.k(0.0, o), l.k(2.0, o));
            let period = l.op2("multiply", F, b, two, o)?;
            let shifted = l.op2("subtract", F, a, b, o)?;
            let phase = l.op2("divide", F, shifted, period, o)?;
            let phase = l.op1("fract", F, phase, o)?;
            let ramp = l.op2("multiply", F, phase, period, o)?;
            let centered = l.op2("subtract", F, ramp, b, o)?;
            let wave = l.op1("absval", F, centered, o)?;
            l.if_equal(b, zero, zero, wave, F, o)?
        }
        "SINE" => l.op1("sin", F, a, o)?,
        "COSINE" => l.op1("cos", F, a, o)?,
        "TANGENT" => l.op1("tan", F, a, o)?,
        "ARCSINE" => l.op1("asin", F, a, o)?,
        "ARCCOSINE" => l.op1("acos", F, a, o)?,
        "ARCTANGENT" => {
            let one = l.k(1.0, o);
            l.op("atan2", F, &[("iny", Arg::Node(a)), ("inx", Arg::Node(one))], o)?
        }
        "ARCTAN2" => l.op("atan2", F, &[("iny", Arg::Node(a)), ("inx", Arg::Node(b))], o)?,
        "SINH" | "COSH" => {
            let (minus_one, two) = (l.k(-1.0, o), l.k(2.0, o));
            let flipped = l.op2("multiply", F, a, minus_one, o)?;
            let (up, down) = (l.op1("exp", F, a, o)?, l.op1("exp", F, flipped, o)?);
            let combined = l.op2(if op == "SINH" { "subtract" } else { "add" }, F, up, down, o)?;
            l.op2("divide", F, combined, two, o)?
        }
        "TANH" => {
            let (one, two) = (l.k(1.0, o), l.k(2.0, o));
            let doubled = l.op2("multiply", F, a, two, o)?;
            let e = l.op1("exp", F, doubled, o)?;
            let (top, bottom) = (l.op2("subtract", F, e, one, o)?, l.op2("add", F, e, one, o)?);
            l.op2("divide", F, top, bottom, o)?
        }
        "RADIANS" => {
            let factor = l.k(PI / 180.0, o);
            l.op2("multiply", F, a, factor, o)?
        }
        "DEGREES" => {
            let factor = l.k(180.0 / PI, o);
            l.op2("multiply", F, a, factor, o)?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// `x` with its fraction cut off toward zero, of type `ty`.
fn trunc(l: &mut Lowerer, ty: Type, x: NodeId, o: &str) -> Result<NodeId, Error> {
    let (sign, magnitude) = (l.op1("sign", ty, x, o)?, l.op1("absval", ty, x, o)?);
    let whole = l.op1("floor", ty, magnitude, o)?;
    l.op2("multiply", ty, sign, whole, o)
}

/// Blender's polynomial smooth minimum of `a` and `b` over the distance `c`; the plain minimum when `c` is zero.
fn smooth_min(l: &mut Lowerer, a: NodeId, b: NodeId, c: NodeId, o: &str) -> Result<NodeId, Error> {
    let (zero, sixth) = (l.k(0.0, o), l.k(1.0 / 6.0, o));
    let difference = l.op2("subtract", F, a, b, o)?;
    let distance = l.op1("absval", F, difference, o)?;
    let within = l.op2("subtract", F, c, distance, o)?;
    let within = l.op2("max", F, within, zero, o)?;
    let h = l.op2("divide", F, within, c, o)?;
    let h2 = l.op2("multiply", F, h, h, o)?;
    let h3 = l.op2("multiply", F, h2, h, o)?;
    let scale = l.op2("multiply", F, c, sixth, o)?;
    let drop = l.op2("multiply", F, h3, scale, o)?;
    let smaller = l.op2("min", F, a, b, o)?;
    let smooth = l.op2("subtract", F, smaller, drop, o)?;
    l.if_equal(c, zero, smaller, smooth, F, o)
}

/// How many of the Vector Math node's operands (`Vector`, `Vector_001`, `Vector_002`) `op` reads, and whether its
/// result is the `Value` output rather than `Vector`; `None` for an operation this version does not know.
fn vector_shape(op: &str) -> Option<(usize, bool)> {
    Some(match op {
        "NORMALIZE" | "ABSOLUTE" | "FLOOR" | "CEIL" | "FRACTION" | "SINE" | "COSINE" | "TANGENT" | "SCALE" => {
            (1, false)
        }
        "ADD" | "SUBTRACT" | "MULTIPLY" | "DIVIDE" | "CROSS_PRODUCT" | "PROJECT" | "REFLECT" | "REFRACT"
        | "MINIMUM" | "MAXIMUM" | "MODULO" | "SNAP" => (2, false),
        "MULTIPLY_ADD" | "FACEFORWARD" | "WRAP" => (3, false),
        "LENGTH" => (1, true),
        "DOT_PRODUCT" | "DISTANCE" => (2, true),
        _ => return None,
    })
}

/// The Vector Math node: output `Vector` or `Value`, whichever `operation` produces; the other is Blender's
/// disabled socket and reads as zero.
pub fn vector_math(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let op = node.text("operation").unwrap_or("ADD");
    let o = node.name.as_str();
    let out_ty = if output == "Value" { F } else { V };
    let a = l.input(node, "Vector", V)?;
    let Some((arity, scalar)) = vector_shape(op) else {
        l.warn(node, format!("operation {op} not lowered, the first value passes through"));
        return l.convert(a, out_ty, o);
    };
    if (output == "Value") != scalar {
        l.warn(node, format!("output {output} is not produced by {op}, zero"));
        return Ok(l.e.constant(Value::zero(out_ty), o));
    }
    let mut operands = [a; 3];
    for (slot, socket) in operands.iter_mut().zip(["Vector", "Vector_001", "Vector_002"]).take(arity).skip(1) {
        *slot = l.input(node, socket, V)?;
    }
    let [a, b, c] = operands;
    let result = match op {
        "ADD" => l.op2("add", V, a, b, o)?,
        "SUBTRACT" => l.op2("subtract", V, a, b, o)?,
        "MULTIPLY" => l.op2("multiply", V, a, b, o)?,
        "DIVIDE" => l.op2("divide", V, a, b, o)?,
        "MULTIPLY_ADD" => {
            let product = l.op2("multiply", V, a, b, o)?;
            l.op2("add", V, product, c, o)?
        }
        "CROSS_PRODUCT" => l.op2("crossproduct", V, a, b, o)?,
        "PROJECT" => {
            let (along, length) = (l.op2("dotproduct", F, a, b, o)?, l.op2("dotproduct", F, b, b, o)?);
            let ratio = l.op2("divide", F, along, length, o)?;
            l.op2("multiply", V, b, ratio, o)?
        }
        "REFLECT" => {
            let normal = l.op1("normalize", V, b, o)?;
            l.op("reflect", V, &[("in", Arg::Node(a)), ("normal", Arg::Node(normal))], o)?
        }
        "REFRACT" => {
            let ior = l.input(node, "Scale", F)?;
            let normal = l.op1("normalize", V, b, o)?;
            let inputs = [("in", Arg::Node(a)), ("normal", Arg::Node(normal)), ("ior", Arg::Node(ior))];
            l.op("refract", V, &inputs, o)?
        }
        "FACEFORWARD" => {
            let (zero, minus_one) = (l.k(0.0, o), l.k(-1.0, o));
            let facing = l.op2("dotproduct", F, b, c, o)?;
            let flipped = l.op2("multiply", V, a, minus_one, o)?;
            l.if_greater(zero, facing, a, flipped, V, o)?
        }
        "DOT_PRODUCT" => l.op2("dotproduct", F, a, b, o)?,
        "DISTANCE" => l.op2("distance", F, a, b, o)?,
        "LENGTH" => l.op1("magnitude", F, a, o)?,
        "SCALE" => {
            let scale = l.input(node, "Scale", F)?;
            l.op2("multiply", V, a, scale, o)?
        }
        "NORMALIZE" => l.op1("normalize", V, a, o)?,
        "ABSOLUTE" => l.op1("absval", V, a, o)?,
        "MINIMUM" => l.op2("min", V, a, b, o)?,
        "MAXIMUM" => l.op2("max", V, a, b, o)?,
        "FLOOR" => l.op1("floor", V, a, o)?,
        "CEIL" => l.op1("ceil", V, a, o)?,
        "FRACTION" => l.op1("fract", V, a, o)?,
        "MODULO" => {
            let quotient = l.op2("divide", V, a, b, o)?;
            let whole = trunc(l, V, quotient, o)?;
            let multiple = l.op2("multiply", V, b, whole, o)?;
            l.op2("subtract", V, a, multiple, o)?
        }
        "WRAP" => {
            let offset = l.op2("subtract", V, a, c, o)?;
            let range = l.op2("subtract", V, b, c, o)?;
            let wrapped = l.op2("modulo", V, offset, range, o)?;
            l.op2("add", V, c, wrapped, o)?
        }
        "SNAP" => {
            let quotient = l.op2("divide", V, a, b, o)?;
            let steps = l.op1("floor", V, quotient, o)?;
            l.op2("multiply", V, steps, b, o)?
        }
        "SINE" => l.op1("sin", V, a, o)?,
        "COSINE" => l.op1("cos", V, a, o)?,
        _ => l.op1("tan", V, a, o)?,
    };
    Ok(result)
}

/// The Clamp node: `MINMAX` limits `Value` to `Min` and `Max` as given; `RANGE` to the lower and the higher of them.
pub fn clamp(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let value = l.input(node, "Value", F)?;
    let (min, max) = (l.input(node, "Min", F)?, l.input(node, "Max", F)?);
    let (low, high) = if node.text("clamp_type") == Some("RANGE") {
        (l.op2("min", F, min, max, o)?, l.op2("max", F, min, max, o)?)
    } else {
        (min, max)
    };
    l.op("clamp", F, &[("in", Arg::Node(value)), ("low", Arg::Node(low)), ("high", Arg::Node(high))], o)
}

/// The Map Range node over floats (`Result`) or vectors (`Vector`, `_FLOAT3` sockets): the interpolation from
/// `From Min..From Max` to `To Min..To Max`, limited to the target range by `clamp`.
pub fn map_range(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let vector = node.text("data_type") == Some("FLOAT_VECTOR");
    let (ty, produced, sockets) = if vector {
        (V, "Vector", ["Vector", "From_Min_FLOAT3", "From_Max_FLOAT3", "To_Min_FLOAT3", "To_Max_FLOAT3"])
    } else {
        (F, "Result", ["Value", "From Min", "From Max", "To Min", "To Max"])
    };
    if output != produced {
        let data_type = node.text("data_type").unwrap_or("FLOAT");
        l.warn(node, format!("output {output} is not produced over {data_type}, zero"));
        let out_ty = if output == "Vector" { V } else { F };
        return Ok(l.e.constant(Value::zero(out_ty), o));
    }
    let mut interpolation = node.text("interpolation_type").unwrap_or("LINEAR");
    if vector && interpolation != "LINEAR" {
        l.warn(node, format!("{interpolation} over vectors written as Linear"));
        interpolation = "LINEAR";
    }
    let [v, from_min, from_max, to_min, to_max] = sockets.map(|socket| l.input(node, socket, ty));
    let (v, from_min, from_max, to_min, to_max) = (v?, from_min?, from_max?, to_min?, to_max?);
    let result = match interpolation {
        "STEPPED" => {
            let steps = l.input(node, "Steps", ty)?;
            let one = l.k_of(1.0, ty, o);
            let t = unit_position(l, ty, v, from_min, from_max, o)?;
            let levels = l.op2("add", ty, steps, one, o)?;
            let scaled = l.op2("multiply", ty, t, levels, o)?;
            let stepped = l.op1("floor", ty, scaled, o)?;
            let t = l.op2("divide", ty, stepped, steps, o)?;
            lerp(l, ty, t, to_min, to_max, o)?
        }
        "SMOOTHSTEP" => {
            let inputs = [("in", Arg::Node(v)), ("low", Arg::Node(from_min)), ("high", Arg::Node(from_max))];
            let t = l.op("smoothstep", ty, &inputs, o)?;
            lerp(l, ty, t, to_min, to_max, o)?
        }
        "SMOOTHERSTEP" => {
            let (six, fifteen, ten) = (l.k_of(6.0, ty, o), l.k_of(15.0, ty, o), l.k_of(10.0, ty, o));
            let t = unit_position(l, ty, v, from_min, from_max, o)?;
            let t = l.clamp01(t, ty, o)?;
            let squared = l.op2("multiply", ty, t, t, o)?;
            let cubed = l.op2("multiply", ty, squared, t, o)?;
            let times_six = l.op2("multiply", ty, t, six, o)?;
            let inner = l.op2("subtract", ty, times_six, fifteen, o)?;
            let inner = l.op2("multiply", ty, t, inner, o)?;
            let polynomial = l.op2("add", ty, inner, ten, o)?;
            let t = l.op2("multiply", ty, cubed, polynomial, o)?;
            lerp(l, ty, t, to_min, to_max, o)?
        }
        _ => {
            let inputs = [
                ("in", Arg::Node(v)),
                ("inlow", Arg::Node(from_min)),
                ("inhigh", Arg::Node(from_max)),
                ("outlow", Arg::Node(to_min)),
                ("outhigh", Arg::Node(to_max)),
            ];
            l.op("remap", ty, &inputs, o)?
        }
    };
    if node.flag("clamp") {
        let (low, high) = (l.op2("min", ty, to_min, to_max, o)?, l.op2("max", ty, to_min, to_max, o)?);
        return l.op("clamp", ty, &[("in", Arg::Node(result)), ("low", Arg::Node(low)), ("high", Arg::Node(high))], o);
    }
    Ok(result)
}

/// Where `v` lies between `from_min` (zero) and `from_max` (one).
fn unit_position(
    l: &mut Lowerer,
    ty: Type,
    v: NodeId,
    from_min: NodeId,
    from_max: NodeId,
    o: &str,
) -> Result<NodeId, Error> {
    let offset = l.op2("subtract", ty, v, from_min, o)?;
    let range = l.op2("subtract", ty, from_max, from_min, o)?;
    l.op2("divide", ty, offset, range, o)
}

/// `to_min` at `t` = 0 to `to_max` at `t` = 1.
fn lerp(l: &mut Lowerer, ty: Type, t: NodeId, to_min: NodeId, to_max: NodeId, o: &str) -> Result<NodeId, Error> {
    let span = l.op2("subtract", ty, to_max, to_min, o)?;
    let scaled = l.op2("multiply", ty, t, span, o)?;
    l.op2("add", ty, to_min, scaled, o)
}
