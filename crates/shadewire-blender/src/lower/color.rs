//! The Mix node (every blend mode, in Blender's formulas), Separate and Combine Color and XYZ, Invert, Gamma,
//! Brightness/Contrast and Hue/Saturation/Value as MaterialX math.

use shadewire::{Error, NodeId, Type, Value};
use shadewire_nodes::Arg;

use super::{Lowerer, missing_output};
use crate::graph::FlatNode;

const F: Type = Type::Float;
const V: Type = Type::Vector3;
const C: Type = Type::Color3;

/// The Mix node: `Result_Float`, `Result_Vector` or `Result_Color`, whichever `data_type` produces; the other two
/// are Blender's disabled sockets and read as zero. Only `RGBA` has blend modes, and only it clamps its result.
pub fn mix(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let data_type = node.text("data_type").unwrap_or("FLOAT");
    let (ty, produced, a_socket, b_socket) = match data_type {
        "RGBA" => (C, "Result_Color", "A_Color", "B_Color"),
        "VECTOR" => (V, "Result_Vector", "A_Vector", "B_Vector"),
        _ => (F, "Result_Float", "A_Float", "B_Float"),
    };
    if output != produced {
        let out_ty = match output {
            "Result_Float" => F,
            "Result_Vector" => V,
            "Result_Color" => C,
            _ => return Err(missing_output(node, output)),
        };
        l.warn(node, format!("output {output} is not produced over {data_type}, zero"));
        return Ok(l.e.constant(Value::zero(out_ty), o));
    }
    let per_component = ty == V && node.text("factor_mode") == Some("NON_UNIFORM");
    let (fac_socket, fac_ty) = if per_component { ("Factor_Vector", V) } else { ("Factor_Float", F) };
    let mut fac = l.input(node, fac_socket, fac_ty)?;
    if node.flag("clamp_factor") {
        fac = l.clamp01(fac, fac_ty, o)?;
    }
    let (a, b) = (l.input(node, a_socket, ty)?, l.input(node, b_socket, ty)?);
    if ty != C {
        return mix_of(l, ty, a, b, fac, o);
    }
    let result = blend(l, node, a, b, fac)?;
    if node.flag("clamp_result") { l.clamp01(result, C, o) } else { Ok(result) }
}

/// `mix(bg = a, fg = b, mix = fac)`: `b` at `fac` = 1, `a` at `fac` = 0.
fn mix_of(l: &mut Lowerer, ty: Type, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    l.op("mix", ty, &[("bg", Arg::Node(a)), ("fg", Arg::Node(b)), ("mix", Arg::Node(fac))], o)
}

/// `mix(a, op(a, b), fac)` for the two-input color operation `op` (`multiply`, `add`, `min`, ...).
fn mixed_with(l: &mut Lowerer, op: &str, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    let result = l.op2(op, C, a, b, o)?;
    mix_of(l, C, a, result, fac, o)
}

/// The colors `a` and `b` blended at the float `fac` by the node's `blend_type`.
fn blend(l: &mut Lowerer, node: &FlatNode, a: NodeId, b: NodeId, fac: NodeId) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let kind = node.text("blend_type").unwrap_or("MIX");
    // The library has `screen`, `plus`, `minus` and `difference` as GLSL source only, so those are written out;
    // `overlay` is the one compositing node whose definition is a node graph.
    match kind {
        "MIX" => mix_of(l, C, a, b, fac, o),
        "MULTIPLY" => mixed_with(l, "multiply", a, b, fac, o),
        "ADD" => mixed_with(l, "add", a, b, fac, o),
        "SUBTRACT" => mixed_with(l, "subtract", a, b, fac, o),
        "DARKEN" => mixed_with(l, "min", a, b, fac, o),
        "LIGHTEN" => mixed_with(l, "max", a, b, fac, o),
        "DIFFERENCE" => {
            let difference = l.op2("subtract", C, a, b, o)?;
            let distance = l.op1("absval", C, difference, o)?;
            mix_of(l, C, a, distance, fac, o)
        }
        "OVERLAY" => {
            let inputs = [("bg", Arg::Node(a)), ("fg", Arg::Node(b)), ("mix", Arg::Node(fac))];
            l.op("overlay", C, &inputs, o)
        }
        "SCREEN" => screen(l, a, b, fac, o),
        "DIVIDE" => divide(l, a, b, fac, o),
        "SOFT_LIGHT" => soft_light(l, a, b, fac, o),
        "LINEAR_LIGHT" => linear_light(l, a, b, fac, o),
        "EXCLUSION" => exclusion(l, a, b, fac, o),
        "BURN" => burn(l, a, b, fac, o),
        "DODGE" => dodge(l, a, b, fac, o),
        // Which of hue, saturation and value come from B, and whose saturation must be above zero for the blend to
        // change anything (Blender leaves A as it is otherwise).
        "HUE" => hsv_blend(l, a, b, fac, [true, false, false], Some(true), o),
        "SATURATION" => hsv_blend(l, a, b, fac, [false, true, false], Some(false), o),
        "VALUE" => hsv_blend(l, a, b, fac, [false, false, true], None, o),
        "COLOR" => hsv_blend(l, a, b, fac, [true, true, false], Some(true), o),
        _ => {
            l.warn(node, format!("blend {kind} not lowered, written as Mix"));
            mix_of(l, C, a, b, fac, o)
        }
    }
}

fn screen(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    let one = l.k3(1.0, C, o);
    let (inv_a, inv_b) = (l.op2("subtract", C, one, a, o)?, l.op2("subtract", C, one, b, o)?);
    let both = l.op2("multiply", C, inv_a, inv_b, o)?;
    let screen = l.op2("subtract", C, one, both, o)?;
    mix_of(l, C, a, screen, fac, o)
}

/// `a / b` per channel, `a` where `b` is zero.
fn divide(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    let quotient = l.per_channel(a, o, &mut |l, ai, i| {
        let bi = l.extract(b, i as u32, o)?;
        let zero = l.k(0.0, o);
        let ratio = l.op2("divide", F, ai, bi, o)?;
        l.if_equal(bi, zero, ai, ratio, F, o)
    })?;
    mix_of(l, C, a, quotient, fac, o)
}

fn soft_light(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    let one = l.k3(1.0, C, o);
    let (inv_a, inv_b) = (l.op2("subtract", C, one, a, o)?, l.op2("subtract", C, one, b, o)?);
    let screened = l.op2("multiply", C, inv_b, inv_a, o)?;
    let screen = l.op2("subtract", C, one, screened, o)?;
    let shadow = l.op2("multiply", C, inv_a, b, o)?;
    let shadow = l.op2("multiply", C, shadow, a, o)?;
    let light = l.op2("multiply", C, a, screen, o)?;
    let soft = l.op2("add", C, shadow, light, o)?;
    mix_of(l, C, a, soft, fac, o)
}

/// `a + fac * (2b - 1)`: the factor is inside, not a mix.
fn linear_light(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    let (one, two) = (l.k3(1.0, C, o), l.k3(2.0, C, o));
    let fac = l.convert(fac, C, o)?;
    let doubled = l.op2("multiply", C, b, two, o)?;
    let signed = l.op2("subtract", C, doubled, one, o)?;
    let push = l.op2("multiply", C, fac, signed, o)?;
    l.op2("add", C, a, push, o)
}

fn exclusion(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    let two = l.k3(2.0, C, o);
    let sum = l.op2("add", C, a, b, o)?;
    let product = l.op2("multiply", C, a, b, o)?;
    let doubled = l.op2("multiply", C, product, two, o)?;
    let excluded = l.op2("subtract", C, sum, doubled, o)?;
    mix_of(l, C, a, excluded, fac, o)
}

/// Color burn per channel; the factor is inside, not a mix.
fn burn(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    l.per_channel(a, o, &mut |l, ai, i| {
        let bi = l.extract(b, i as u32, o)?;
        let (zero, one) = (l.k(0.0, o), l.k(1.0, o));
        let lacking = l.op2("subtract", F, one, bi, o)?;
        let scaled = l.op2("multiply", F, fac, lacking, o)?;
        let t = l.op2("subtract", F, one, scaled, o)?;
        let dark = l.op2("subtract", F, one, ai, o)?;
        let ratio = l.op2("divide", F, dark, t, o)?;
        let burnt = l.op2("subtract", F, one, ratio, o)?;
        let burnt = l.clamp01(burnt, F, o)?;
        l.if_greater(t, zero, burnt, zero, F, o)
    })
}

/// Color dodge per channel; the factor is inside, not a mix.
fn dodge(l: &mut Lowerer, a: NodeId, b: NodeId, fac: NodeId, o: &str) -> Result<NodeId, Error> {
    l.per_channel(a, o, &mut |l, ai, i| {
        let bi = l.extract(b, i as u32, o)?;
        let (zero, one) = (l.k(0.0, o), l.k(1.0, o));
        let scaled = l.op2("multiply", F, fac, bi, o)?;
        let t = l.op2("subtract", F, one, scaled, o)?;
        let ratio = l.op2("divide", F, ai, t, o)?;
        let dodged = l.op2("min", F, ratio, one, o)?;
        let r = l.if_greater(t, zero, dodged, one, F, o)?;
        l.if_equal(ai, zero, zero, r, F, o)
    })
}

/// `a` with the hue, saturation or value flagged in `from_b` taken from `b`, mixed in at `fac`. `guard` names the
/// color (`true` for B) whose saturation must be above zero, `a` itself being the result when it is not.
fn hsv_blend(
    l: &mut Lowerer,
    a: NodeId,
    b: NodeId,
    fac: NodeId,
    from_b: [bool; 3],
    guard: Option<bool>,
    o: &str,
) -> Result<NodeId, Error> {
    let (hsv_a, hsv_b) = (l.op1("rgbtohsv", C, a, o)?, l.op1("rgbtohsv", C, b, o)?);
    let pick =
        |l: &mut Lowerer, channel: usize| l.extract(if from_b[channel] { hsv_b } else { hsv_a }, channel as u32, o);
    let (h, s, v) = (pick(l, 0)?, pick(l, 1)?, pick(l, 2)?);
    let joined = l.combine3(h, s, v, C, o)?;
    let mut rgb = l.op1("hsvtorgb", C, joined, o)?;
    if let Some(of_b) = guard {
        let saturation = l.extract(if of_b { hsv_b } else { hsv_a }, 1, o)?;
        let zero = l.k(0.0, o);
        rgb = l.if_greater(saturation, zero, rgb, a, C, o)?;
    }
    mix_of(l, C, a, rgb, fac, o)
}

/// The channel of an RGB or XYZ output, by its identifier.
fn channel_of(node: &FlatNode, output: &str, names: [&str; 3]) -> Result<u32, Error> {
    match names.iter().position(|name| *name == output) {
        Some(index) => Ok(index as u32),
        None => Err(missing_output(node, output)),
    }
}

/// Separate Color: the `Red`, `Green` or `Blue` output of `Color` read as RGB or HSV (`mode`); Blender's HSL is
/// read as HSV, with a warning.
pub fn separate_color(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let index = channel_of(node, output, ["Red", "Green", "Blue"])?;
    let mode = node.text("mode").unwrap_or("RGB");
    if mode == "HSL" {
        l.warn(node, "HSL read as HSV");
    }
    let color = l.input(node, "Color", C)?;
    let color = if mode == "RGB" { color } else { l.op1("rgbtohsv", C, color, o)? };
    l.extract(color, index, o)
}

/// Combine Color: `Color` from `Red`, `Green` and `Blue` read as RGB or HSV (`mode`); HSL is read as HSV, with a
/// warning.
pub fn combine_color(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let mode = node.text("mode").unwrap_or("RGB");
    if mode == "HSL" {
        l.warn(node, "HSL read as HSV");
    }
    let (r, g, b) = (l.input(node, "Red", F)?, l.input(node, "Green", F)?, l.input(node, "Blue", F)?);
    let color = l.combine3(r, g, b, C, o)?;
    if mode == "RGB" { Ok(color) } else { l.op1("hsvtorgb", C, color, o) }
}

/// Separate XYZ: the `X`, `Y` or `Z` output of `Vector`.
pub fn separate_xyz(l: &mut Lowerer, node: &FlatNode, output: &str) -> Result<NodeId, Error> {
    let index = channel_of(node, output, ["X", "Y", "Z"])?;
    let vector = l.input(node, "Vector", V)?;
    l.extract(vector, index, &node.name)
}

/// Combine XYZ: `Vector` from `X`, `Y` and `Z`.
pub fn combine_xyz(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let (x, y, z) = (l.input(node, "X", F)?, l.input(node, "Y", F)?, l.input(node, "Z", F)?);
    l.combine3(x, y, z, V, &node.name)
}

/// Invert: `Color` turned into one minus itself, as far as `Fac`.
pub fn invert(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let (fac, color) = (l.input(node, "Fac", F)?, l.input(node, "Color", C)?);
    let inverted = l.op1("invert", C, color, o)?;
    mix_of(l, C, color, inverted, fac, o)
}

/// Gamma: `Color` to the power `Gamma`, from zero up.
pub fn gamma(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let (color, exponent) = (l.input(node, "Color", C)?, l.input(node, "Gamma", F)?);
    let zero = l.k3(0.0, C, o);
    let positive = l.op2("max", C, color, zero, o)?;
    l.op2("power", C, positive, exponent, o)
}

/// Brightness/Contrast: `Color · (1 + Contrast) + Bright − Contrast / 2`, from zero up.
pub fn bright_contrast(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let color = l.input(node, "Color", C)?;
    let (bright, contrast) = (l.input(node, "Bright", F)?, l.input(node, "Contrast", F)?);
    let (one, two, zero) = (l.k(1.0, o), l.k(2.0, o), l.k3(0.0, C, o));
    let gain = l.op2("add", F, one, contrast, o)?;
    let half = l.op2("divide", F, contrast, two, o)?;
    let offset = l.op2("subtract", F, bright, half, o)?;
    let scaled = l.op2("multiply", C, color, gain, o)?;
    let shifted = l.op2("add", C, scaled, offset, o)?;
    l.op2("max", C, shifted, zero, o)
}

/// Hue/Saturation/Value: `Color` with its hue moved by `Hue − 0.5` and its saturation and value scaled, mixed in at
/// `Fac`, from zero up.
pub fn hue_saturation(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<NodeId, Error> {
    let o = node.name.as_str();
    let color = l.input(node, "Color", C)?;
    let (hue, saturation) = (l.input(node, "Hue", F)?, l.input(node, "Saturation", F)?);
    let (value, fac) = (l.input(node, "Value", F)?, l.input(node, "Fac", F)?);
    let (half, zero) = (l.k(0.5, o), l.k3(0.0, C, o));
    let shift = l.op2("subtract", F, hue, half, o)?;
    let amount = l.combine3(shift, saturation, value, V, o)?;
    let adjusted = l.op("hsvadjust", C, &[("in", Arg::Node(color)), ("amount", Arg::Node(amount))], o)?;
    let mixed = mix_of(l, C, color, adjusted, fac, o)?;
    l.op2("max", C, mixed, zero, o)
}
