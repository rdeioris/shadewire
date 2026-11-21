//! Expressions that only route values: reroutes, static and engine switches, material function calls with their
//! inputs and outputs.

use shadewire::{Error, NodeId};

use super::params::{function_input_position, static_switch_value};
use super::{Lowerer, short};
use crate::document::Expression;

/// The value an input of `x` routes, zero with a warning when nothing is linked to it.
fn routed(l: &mut Lowerer, f: usize, x: &Expression, index: usize) -> Result<NodeId, Error> {
    match l.input_at(f, x, index)? {
        Some(id) => Ok(id),
        None => {
            l.warn(f, x, "nothing is linked, read as 0");
            Ok(l.k(0.0, &l.origin(f, x)))
        }
    }
}

pub fn reroute(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    routed(l, f, x, 0)
}

/// A named reroute usage reads its declaration.
pub fn named_reroute(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    match x.text("Declaration") {
        Some(declaration) => l.output(f, declaration, 0),
        None => {
            l.warn(f, x, "no declaration, read as 0");
            Ok(l.k(0.0, &l.origin(f, x)))
        }
    }
}

/// A static switch takes the branch its value picks; the other is never compiled.
pub fn static_switch(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let branch = if static_switch_value(l, f, x) { "True" } else { "False" };
    match l.input(f, x, branch)? {
        Some(id) => Ok(id),
        None => {
            l.warn(f, x, format!("{branch} is not linked, read as 0"));
            Ok(l.k(0.0, &l.origin(f, x)))
        }
    }
}

/// Quality, feature level and shading path switches: the branch the highest-quality, deferred, SM6 renderer
/// takes; a warning names the other linked branches it drops.
pub fn engine_switch(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let order: &[&str] = match super::short(&x.class) {
        "QualitySwitch" => &["Default", "High", "Epic", "Medium", "Low"],
        "FeatureLevelSwitch" => &["SM6", "SM5", "Default", "ES3_1"],
        _ => &["Deferred", "Default", "Forward", "Mobile"],
    };
    let linked = |name: &str| x.input(name).is_some_and(|i| i.from.is_some());
    let Some(&taken) = order.iter().find(|&&name| linked(name)) else {
        l.warn(f, x, "no branch is linked, read as 0");
        return Ok(l.k(0.0, &l.origin(f, x)));
    };
    let dropped: Vec<&str> =
        x.inputs.iter().filter(|i| i.from.is_some() && i.name != taken).map(|i| i.name.as_str()).collect();
    if !dropped.is_empty() {
        l.warn(f, x, format!("{taken} taken, {} dropped", dropped.join(", ")));
    }
    Ok(l.input(f, x, taken)?.expect("the taken branch is linked"))
}

/// The input a Switch picks: the one its value indexes (`floor`, in the order of its `Inputs`), else `Default`. The
/// value is the switch's constant when nothing is linked, or a scalar parameter's (the instance's override, else its
/// default): constant for a material, so the switch resolves when the material is lowered. `None` when the value
/// comes from anything else.
pub(super) fn switch_input(l: &Lowerer, f: usize, x: &Expression) -> Option<String> {
    let value = match x.input("SwitchValue").and_then(|i| i.from.clone()) {
        None => x.number("ConstSwitchValue").unwrap_or(0.0),
        Some((name, _)) => {
            let source = l.expression(f, &name)?;
            if short(&source.class) != "ScalarParameter" {
                return None;
            }
            let parameter = source.text("ParameterName").unwrap_or("");
            l.params.scalar.get(parameter).copied().or_else(|| source.number("DefaultValue")).unwrap_or(0.0)
        }
    };
    let inputs: Vec<&str> =
        x.inputs.iter().map(|i| i.name.as_str()).filter(|n| *n != "SwitchValue" && *n != "Default").collect();
    let index = value.floor();
    match (index >= 0.0).then(|| inputs.get(index as usize)).flatten() {
        Some(name) => Some(name.to_string()),
        None => Some("Default".to_string()),
    }
}

/// Switch: the input its (constant) value picks; an unlinked Default is its constant. A value that is not constant
/// for the material is not supported and reads the Default, with a warning.
pub fn switch(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let origin = l.origin(f, x);
    let picked = match switch_input(l, f, x) {
        Some(name) => name,
        None => {
            l.warn(f, x, "the switch value is not a constant or a scalar parameter, Default is taken");
            "Default".to_string()
        }
    };
    match l.input(f, x, &picked)? {
        Some(id) => Ok(id),
        None => Ok(l.k(x.number("ConstDefault").unwrap_or(0.0), &origin)),
    }
}

/// Output `out` of a function call: the matching FunctionOutput of the function, in the call's own frame.
pub fn function_call<'a>(l: &mut Lowerer<'a>, f: usize, x: &'a Expression, out: usize) -> Result<NodeId, Error> {
    let Some(frame) = l.call_frame(f, x)? else { return Ok(l.k(0.0, &l.origin(f, x))) };
    let list = l.function_expressions(frame).unwrap_or(&[]);
    let Some(output) = l.call_output(x, list, out) else {
        l.warn(f, x, format!("the function has no output {out}, read as 0"));
        return Ok(l.k(0.0, &l.origin(f, x)));
    };
    let name = output.name.clone();
    l.output(frame, &name, 0)
}

pub fn function_output(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    routed(l, f, x, 0)
}

/// A function input reads what the call links to it, else its preview (when it is the default), cast to its
/// input type.
pub fn function_input(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let origin = l.origin(f, x);
    let width = match x.text("InputType").unwrap_or("FunctionInput_Vector3") {
        "FunctionInput_Scalar" => Some(1),
        "FunctionInput_Vector2" => Some(2),
        "FunctionInput_Vector3" => Some(3),
        "FunctionInput_Vector4" => Some(4),
        "FunctionInput_StaticBool" | "FunctionInput_Bool" => Some(1),
        other => {
            l.warn(f, x, format!("{other} inputs are not read, 0"));
            None
        }
    };
    let Some(width) = width else { return Ok(l.k(0.0, &origin)) };
    let linked = match (l.caller(f), function_input_position(l, f, x)) {
        (Some((caller, call)), Some(position)) => l.input_at(caller, call, position)?,
        _ => None,
    };
    let value = match linked {
        Some(id) => id,
        None if x.flag("bUsePreviewValueAsDefault") => match l.input(f, x, "Preview")? {
            Some(id) => id,
            None => {
                let preview = x.numbers("PreviewValue").unwrap_or_else(|| vec![0.0; 4]);
                l.kv(&preview[..width.min(preview.len())], &origin)
            }
        },
        None => {
            l.warn(f, x, "nothing is linked to the call's input and the preview is not the default, read as 0");
            l.zero(width, &origin)
        }
    };
    l.cast(value, width, &origin)
}
