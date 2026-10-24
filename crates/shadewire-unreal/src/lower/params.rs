//! Parameters and settings: an instance chain's overrides applied over its base material, scalar and vector
//! parameters as uniforms named after them, static switches as the values they hold.

use std::collections::HashMap;

use serde_json::Value as Json;
use shadewire::{Error, NodeId, Value};

use super::Lowerer;
use super::short;
use crate::document::{Document, Expression, Material, at, numbers};

/// What a material reads from its settings and its instances' overrides.
#[derive(Clone, Debug, Default)]
pub struct Params {
    pub scalar: HashMap<String, f32>,
    pub vector: HashMap<String, [f32; 4]>,
    /// Texture parameter name to texture path.
    pub texture: HashMap<String, String>,
    pub switch: HashMap<String, bool>,
    pub settings: Settings,
}

#[derive(Clone, Debug, Default)]
pub struct Settings {
    pub domain: String,
    pub blend_mode: String,
    pub shading_models: Vec<String>,
    pub opacity_mask_clip: f32,
    pub tangent_space_normal: bool,
    pub use_attributes: bool,
}

/// The base material `material` builds on, the parameters its chain sets, and the warnings that raised.
pub fn resolve<'a>(doc: &'a Document, material: &'a Material) -> Result<(&'a Material, Params, Vec<String>), Error> {
    let mut params = Params::default();
    let mut warnings = Vec::new();
    let mut chain = vec![material];
    let mut current = material;
    while let Some(parent) = &current.parent {
        let Some(next) = doc.materials.iter().chain(&doc.parents).find(|m| &m.path == parent) else {
            return Err(Error::BadValue {
                at: at(&current.name),
                message: format!("parent {parent} is not in the document"),
            });
        };
        if chain.iter().any(|m| m.path == next.path) {
            return Err(Error::Cycle { at: at(&current.name), node: next.name.clone() });
        }
        chain.push(next);
        current = next;
    }
    let base = current;
    if base.is_instance() {
        return Err(Error::BadValue {
            at: at(&material.name),
            message: format!("{} has no base material", material.name),
        });
    }

    let s = &base.settings;
    let text = |key: &str, default: &str| s.get(key).and_then(Json::as_str).unwrap_or(default).to_string();
    params.settings = Settings {
        domain: text("MaterialDomain", "MD_Surface"),
        blend_mode: text("BlendMode", "BLEND_Opaque"),
        shading_models: match s.get("ShadingModels").and_then(Json::as_array) {
            Some(list) => list.iter().filter_map(Json::as_str).map(str::to_string).collect(),
            None => vec![text("ShadingModel", "MSM_DefaultLit")],
        },
        opacity_mask_clip: s.get("OpacityMaskClipValue").and_then(Json::as_f64).unwrap_or(0.3333) as f32,
        tangent_space_normal: s.get("bTangentSpaceNormal").and_then(Json::as_bool).unwrap_or(true),
        use_attributes: s.get("bUseMaterialAttributes").and_then(Json::as_bool).unwrap_or(false),
    };

    // Nearest instance first: the first value a name gets is the one it keeps.
    for instance in chain.iter().filter(|m| m.is_instance()) {
        let Some(o) = &instance.overrides else { continue };
        for (name, value) in &o.scalar {
            if let Some(x) = value.as_f64() {
                params.scalar.entry(name.clone()).or_insert(x as f32);
            }
        }
        for (name, value) in &o.vector {
            if let Some(v) = numbers(value).filter(|v| v.len() == 4) {
                params.vector.entry(name.clone()).or_insert([v[0], v[1], v[2], v[3]]);
            }
        }
        for (name, value) in &o.texture {
            if let Some(path) = value.as_str() {
                params.texture.entry(name.clone()).or_insert(path.to_string());
            }
        }
        for (name, value) in &o.static_switch {
            if let Some(b) = value.as_bool() {
                params.switch.entry(name.clone()).or_insert(b);
            }
        }
    }
    // Base property overrides, nearest instance last so it wins.
    for instance in chain.iter().rev().filter(|m| m.is_instance()) {
        let Some(o) = &instance.overrides else { continue };
        let b = &o.base_property_overrides;
        let on = |key: &str| b.get(key).and_then(Json::as_bool).unwrap_or(false);
        let get = |key: &str| b.get(key);
        if on("bOverride_BlendMode")
            && let Some(mode) = get("BlendMode").and_then(Json::as_str)
        {
            params.settings.blend_mode = mode.to_string();
        }
        if on("bOverride_ShadingModel")
            && let Some(model) = get("ShadingModel").and_then(Json::as_str)
        {
            params.settings.shading_models = vec![model.to_string()];
        }
        if on("bOverride_OpacityMaskClipValue")
            && let Some(clip) = get("OpacityMaskClipValue").and_then(Json::as_f64)
        {
            params.settings.opacity_mask_clip = clip as f32;
        }
        for key in ["bOverride_DitheredLODTransition", "bOverride_bIsThinSurface"] {
            if on(key) {
                warnings.push(format!("{} \"{}\": {key} ignored", instance.class, instance.name));
            }
        }
    }
    Ok((base, params, warnings))
}

/// A scalar or vector parameter: one uniform per parameter name, holding its instance value or its default.
pub fn parameter(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let name = x.text("ParameterName").unwrap_or(&x.name).to_string();
    if let Some(&id) = l.uniforms.get(&name) {
        return Ok(id);
    }
    let origin = l.origin(f, x);
    let value = if short(&x.class) == "ScalarParameter" {
        Value::Float(l.params.scalar.get(&name).copied().unwrap_or_else(|| x.number_or("DefaultValue", 0.0)))
    } else {
        let default = x.numbers("DefaultValue").unwrap_or_default();
        let v = l.params.vector.get(&name).copied().unwrap_or_else(|| {
            let at = |i: usize| default.get(i).copied().unwrap_or(0.0);
            [at(0), at(1), at(2), at(3)]
        });
        Value::Vector4(v)
    };
    let id = l.e.authored(&name, value, &origin);
    l.uniforms.insert(name, id);
    Ok(id)
}

/// A static bool read as a number: 1 or 0.
pub fn static_bool_value(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let value = own_static_bool(l, x);
    Ok(l.k(if value { 1.0 } else { 0.0 }, &l.origin(f, x)))
}

/// The value a StaticBool or StaticBoolParameter expression holds, with its instance override.
fn own_static_bool(l: &Lowerer, x: &Expression) -> bool {
    match short(&x.class) {
        "StaticBool" => x.flag("Value"),
        _ => {
            let name = x.text("ParameterName").unwrap_or(&x.name);
            l.params.switch.get(name).copied().unwrap_or_else(|| x.flag("DefaultValue"))
        }
    }
}

/// The static bool linked to `input` of `x` in frame `f`, followed through reroutes and function inputs;
/// `default` when nothing is linked.
pub fn static_bool(l: &mut Lowerer, f: usize, x: &Expression, input: &str, default: bool) -> bool {
    let Some(from) = x.input(input).and_then(|i| i.from.clone()) else { return default };
    static_bool_from(l, f, &from.0, default, 0)
}

fn static_bool_from(l: &mut Lowerer, f: usize, name: &str, default: bool, depth: usize) -> bool {
    let Some(x) = l.expression(f, name) else { return default };
    if depth > 64 {
        return default;
    }
    match short(&x.class) {
        "StaticBool" | "StaticBoolParameter" => own_static_bool(l, x),
        "StaticSwitchParameter" | "StaticSwitch" | "Reroute" | "NamedRerouteDeclaration" => {
            let pick = if short(&x.class) == "Reroute" || short(&x.class) == "NamedRerouteDeclaration" {
                "Input"
            } else if static_switch_value(l, f, x) {
                "True"
            } else {
                "False"
            };
            match x.input(pick).and_then(|i| i.from.clone()) {
                Some(from) => static_bool_from(l, f, &from.0, default, depth + 1),
                None => default,
            }
        }
        "NamedRerouteUsage" => match x.text("Declaration").map(str::to_string) {
            Some(declaration) => static_bool_from(l, f, &declaration, default, depth + 1),
            None => default,
        },
        "FunctionInput" => {
            let Some((caller, call)) = l.caller(f) else { return default };
            let position = function_input_position(l, f, x);
            match position.and_then(|i| call.input_at(i)).and_then(|i| i.from.clone()) {
                Some(from) => static_bool_from(l, caller, &from.0, default, depth + 1),
                None => match x.input("Preview").and_then(|i| i.from.clone()) {
                    Some(from) if x.flag("bUsePreviewValueAsDefault") => {
                        static_bool_from(l, f, &from.0, default, depth + 1)
                    }
                    _ => x.numbers("PreviewValue").and_then(|v| v.first().copied()).is_some_and(|v| v != 0.0),
                },
            }
        }
        _ => default,
    }
}

/// Which branch a static switch takes: its `Value` input, else its parameter (with the instance override), else
/// its default.
pub fn static_switch_value(l: &mut Lowerer, f: usize, x: &Expression) -> bool {
    if short(&x.class) == "StaticSwitchParameter" {
        let name = x.text("ParameterName").unwrap_or(&x.name);
        return l.params.switch.get(name).copied().unwrap_or_else(|| x.flag("DefaultValue"));
    }
    let default = x.flag("DefaultValue");
    static_bool(l, f, x, "Value", default)
}

/// The position of the function input `x` among its function's inputs, which is the index of the call's input.
pub fn function_input_position(l: &Lowerer, f: usize, x: &Expression) -> Option<usize> {
    let list = l.function_expressions(f)?;
    l.sorted("FunctionInput", list).iter().position(|i| i.name == x.name)
}
