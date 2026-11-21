//! The material's pins as a surface: legacy shading models as `open_pbr_surface` (Unlit as `surface_unlit`), with
//! Unreal's defaults for unlinked pins and its blend mode as opacity.

use std::collections::{BTreeSet, HashMap};

use shadewire::{ContextRead, Dag, Error, NodeId, Space, Type, Value};
use shadewire_nodes::Arg;

use super::params::{function_input_position, static_switch_value};
use super::structure::switch_input;
use super::{Lowerer, short};
use crate::document::{Expression, Material};

/// MakeMaterialAttributes inputs whose names are not their pins'.
const ATTRIBUTE_PINS: [(&str, &str); 2] = [("ClearCoat", "CustomData0"), ("ClearCoatRoughness", "CustomData1")];

/// Unreal's value for each attribute nothing sets (`FMaterialAttributeDefinitionMap`): what a blend reads for a pin
/// only one of its sides sets.
const ATTRIBUTE_DEFAULTS: [(&str, Value); 17] = [
    ("BaseColor", Value::Color3([0.0; 3])),
    ("Metallic", Value::Float(0.0)),
    ("Specular", Value::Float(0.5)),
    ("Roughness", Value::Float(0.5)),
    ("Anisotropy", Value::Float(0.0)),
    ("EmissiveColor", Value::Color3([0.0; 3])),
    ("Opacity", Value::Float(1.0)),
    ("OpacityMask", Value::Float(1.0)),
    ("Normal", Value::Vector3([0.0, 0.0, 1.0])),
    ("Tangent", Value::Vector3([1.0, 0.0, 0.0])),
    ("WorldPositionOffset", Value::Vector3([0.0; 3])),
    ("SubsurfaceColor", Value::Color3([1.0; 3])),
    ("CustomData0", Value::Float(1.0)),
    ("CustomData1", Value::Float(0.1)),
    ("AmbientOcclusion", Value::Float(1.0)),
    ("Refraction", Value::Float(1.0)),
    ("PixelDepthOffset", Value::Float(0.0)),
];

/// Pins no legacy model reads.
const IGNORED: [&str; 5] =
    ["WorldPositionOffset", "PixelDepthOffset", "AmbientOcclusion", "Displacement", "ShadingModel"];

/// Where a pin reads from: a frame, an expression output and the mask on the link.
#[derive(Clone)]
struct Source {
    frame: usize,
    from: (String, usize),
    mask: Option<[u8; 4]>,
}

struct Pins<'m> {
    material: &'m Material,
    sources: HashMap<String, Source>,
    /// Pins whose values are already lowered: a BlendMaterialAttributes' blends.
    lowered: HashMap<String, NodeId>,
    /// The Substrate tree the Front Material pin reads, unless it is a Substrate Shading Models node (whose inputs
    /// are then the pins).
    front: Option<Source>,
    /// The shading model a Substrate Shading Models node sets.
    model: Option<String>,
}

impl Pins<'_> {
    fn linked(&self, pin: &str) -> bool {
        self.sources.contains_key(pin) || self.lowered.contains_key(pin)
    }

    /// What `pin` reads, when linked.
    fn read(&self, l: &mut Lowerer, pin: &str) -> Result<Option<NodeId>, Error> {
        if let Some(&id) = self.lowered.get(pin) {
            return Ok(Some(id));
        }
        match self.sources.get(pin) {
            Some(s) => l.link(s.frame, &s.from, s.mask).map(Some),
            None => Ok(None),
        }
    }

    /// What `pin` reads as a float, `default` when unlinked.
    fn float(&self, l: &mut Lowerer, pin: &str, default: f32) -> Result<NodeId, Error> {
        let origin = self.origin(pin);
        match self.read(l, pin)? {
            Some(id) => l.cast(id, 1, &origin),
            None => Ok(l.k(default, &origin)),
        }
    }

    /// What `pin` reads as a `color3`, `default` when unlinked.
    fn color(&self, l: &mut Lowerer, pin: &str, default: [f32; 3]) -> Result<NodeId, Error> {
        let origin = self.origin(pin);
        match self.read(l, pin)? {
            Some(id) => l.color3(id, &origin),
            None => Ok(l.e.constant(Value::Color3(default), &origin)),
        }
    }

    fn origin(&self, pin: &str) -> String {
        format!("{}.{pin}", self.material.name)
    }
}

fn bind(fields: &mut Vec<(String, Arg)>, name: &str, id: NodeId) {
    fields.push((name.to_string(), Arg::Node(id)));
}

pub(super) fn warn(l: &mut Lowerer, material: &Material, what: impl Into<String>) {
    let warning = format!("Material \"{}\": {}", material.name, what.into());
    if !l.warnings.contains(&warning) {
        l.warnings.push(warning);
    }
}

/// The pins `material` links: its own, or the attributes its MaterialAttributes pin reads when it uses material
/// attributes.
fn pins<'m>(l: &mut Lowerer, material: &'m Material) -> Result<Pins<'m>, Error> {
    let mut sources = HashMap::new();
    if !l.params.settings.use_attributes {
        for pin in &material.pins {
            if let Some(from) = &pin.from {
                sources.insert(pin.pin.clone(), Source { frame: 0, from: from.clone(), mask: pin.mask });
            }
        }
        let mut pins = Pins { material, sources, lowered: HashMap::new(), front: None, model: None };
        if let Some(front) = pins.sources.remove("FrontMaterial") {
            front_material(l, &mut pins, front);
        }
        return Ok(pins);
    }
    let from = material.pins.iter().find(|p| p.pin == "MaterialAttributes").and_then(|p| p.from.clone());
    let set = attributes(l, material, 0, from, 0)?;
    Ok(Pins { material, sources: set.links, lowered: set.lowered, front: None, model: None })
}

/// A set of material attributes: pins read from expression outputs, or already lowered.
#[derive(Default)]
struct Attributes {
    links: HashMap<String, Source>,
    lowered: HashMap<String, NodeId>,
}

impl Attributes {
    fn set(&mut self, pin: String, source: Source) {
        self.lowered.remove(&pin);
        self.links.insert(pin, source);
    }

    fn has(&self, pin: &str) -> bool {
        self.links.contains_key(pin) || self.lowered.contains_key(pin)
    }

    /// What `pin` reads, when this set has it.
    fn value(&self, l: &mut Lowerer, pin: &str) -> Result<Option<NodeId>, Error> {
        if let Some(&id) = self.lowered.get(pin) {
            return Ok(Some(id));
        }
        match self.links.get(pin) {
            Some(s) => l.link(s.frame, &s.from, s.mask).map(Some),
            None => Ok(None),
        }
    }
}

/// BreakMaterialAttributes, and GetMaterialAttributes' outputs past the first: output `out` is the attribute it names, from the attributes on its input (followed like
/// the material's own, through reroutes, switches, Make, Set and Blend), or Unreal's default for an attribute nothing
/// sets, in the material or inside a function.
pub fn break_attributes(l: &mut Lowerer, f: usize, x: &Expression, out: usize) -> Result<NodeId, Error> {
    let origin = l.origin(f, x);
    let name = x.outputs.get(out).map(|o| o.name.clone()).unwrap_or_default();
    let pin = attribute_pin(&name);
    let default = ATTRIBUTE_DEFAULTS.iter().find(|(n, _)| *n == pin).map(|(_, v)| v.clone());
    let width = default.as_ref().map_or_else(
        || x.outputs.get(out).and_then(|o| o.mask).map_or(1, |m| m.iter().map(|&c| c as usize).sum::<usize>().max(1)),
        |d| d.ty().component_count(),
    );
    // the material being lowered, for the resolver's warnings: the base material (frame 0's expressions)
    let doc = l.doc;
    let material = doc
        .materials
        .iter()
        .chain(&doc.parents)
        .find(|m| m.expressions.iter().any(|e| l.expression(0, &e.name).is_some_and(|b| std::ptr::eq(b, e))));
    let value = match material {
        Some(material) => {
            let from = x.input_at(0).and_then(|i| i.from.clone());
            let set = attributes(l, material, f, from, 0)?;
            set.value(l, &pin)?
        }
        None => None,
    };
    let id = match (value, default) {
        (Some(id), _) => id,
        (None, Some(default)) => l.e.constant(default, &origin),
        (None, None) => l.zero(width, &origin),
    };
    l.cast(id, width, &origin)
}

/// The pin an attribute input sets: MakeMaterialAttributes names its inputs after the pins, SetMaterialAttributes
/// after their display names ("Base Color"), and both name the clear coat pins by their legacy names.
fn attribute_pin(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    ATTRIBUTE_PINS.iter().find(|(n, _)| *n == name).map_or(name, |(_, p)| p.to_string())
}

/// The attributes the expression output `from` gives: through reroutes and static switches (the branch their value
/// picks), from MakeMaterialAttributes, SetMaterialAttributes and BlendMaterialAttributes. Anything else is a warning
/// and no attributes, so the surface is Unreal's defaults.
fn attributes(
    l: &mut Lowerer,
    material: &Material,
    frame: usize,
    from: Option<(String, usize)>,
    depth: usize,
) -> Result<Attributes, Error> {
    let mut from = from;
    let mut frame = frame;
    for _ in 0..64 {
        if depth > 64 {
            break;
        }
        let Some((name, out)) = from.clone() else { break };
        let Some(x) = l.expression(frame, &name) else { break };
        match short(&x.class) {
            "Reroute" | "NamedRerouteDeclaration" => from = x.input_at(0).and_then(|i| i.from.clone()),
            "NamedRerouteUsage" => from = x.text("Declaration").map(|d| (d.to_string(), 0)),
            "StaticSwitch" | "StaticSwitchParameter" => {
                let branch = if static_switch_value(l, frame, x) { "True" } else { "False" };
                from = x.input(branch).and_then(|i| i.from.clone());
            }
            "MakeMaterialAttributes" => {
                let mut set = Attributes::default();
                for input in &x.inputs {
                    if let Some(link) = &input.from {
                        set.set(attribute_pin(&input.name), Source { frame, from: link.clone(), mask: input.mask });
                    }
                }
                return Ok(set);
            }
            "SetMaterialAttributes" => {
                let base = x.input("MaterialAttributes").and_then(|i| i.from.clone());
                let mut set = attributes(l, material, frame, base, depth + 1)?;
                for input in x.inputs.iter().filter(|i| i.name != "MaterialAttributes") {
                    if let Some(link) = &input.from {
                        set.set(attribute_pin(&input.name), Source { frame, from: link.clone(), mask: input.mask });
                    }
                }
                return Ok(set);
            }
            "BlendMaterialAttributes" => return blend(l, material, frame, x, depth),
            // a function returning material attributes: its output, in the call's frame
            "MaterialFunctionCall" => {
                let Some(called) = l.call_frame(frame, x)? else { break };
                let list = l.function_expressions(called).unwrap_or(&[]);
                let Some(output) = l.call_output(x, list, out) else { break };
                from = output.input_at(0).and_then(|i| i.from.clone());
                frame = called;
            }
            "FunctionOutput" => from = x.input_at(0).and_then(|i| i.from.clone()),
            // GetMaterialAttributes passes its attributes through on output 0
            "GetMaterialAttributes" => from = x.input_at(0).and_then(|i| i.from.clone()),
            "LandscapeLayerBlend" => return landscape_blend(l, material, frame, x, depth),
            "Multiply" | "Add" | "Subtract" | "Divide" | "LinearInterpolate" => {
                return attributes_arithmetic(l, material, frame, x, depth);
            }
            // Unreal picks per component by whether the layer is painted; one material is lowered for the landscape
            "LandscapeLayerSwitch" => {
                from = x.input("LayerUsed").or_else(|| x.input("LayerNotUsed")).and_then(|i| i.from.clone())
            }
            "Switch" => match switch_input(l, frame, x) {
                Some(picked) => from = x.input(&picked).and_then(|i| i.from.clone()),
                None => break,
            },
            // material attributes passed into a function: what the call links to the input, in the caller's frame,
            // else the input's preview
            "FunctionInput" => {
                let linked = match (l.caller(frame), function_input_position(l, frame, x)) {
                    (Some((caller, call)), Some(position)) => {
                        call.input_at(position).and_then(|i| i.from.clone()).map(|link| (caller, link))
                    }
                    _ => None,
                };
                match linked {
                    Some((caller, link)) => {
                        from = Some(link);
                        frame = caller;
                    }
                    None => from = x.input("Preview").and_then(|i| i.from.clone()),
                }
            }
            other => {
                let what = format!("material attributes from {other} are not read, the surface is Unreal's defaults");
                warn(l, material, what);
                break;
            }
        }
    }
    Ok(Attributes::default())
}

/// The expression output `from` leads to material attributes: through reroutes, switches and function boundaries to
/// a node that makes them, or to arithmetic or a landscape blend on them. Decides whether an arithmetic input is
/// attributes (applied per attribute) or a value (the same for every attribute), without warnings.
fn carries_attributes(l: &mut Lowerer, frame: usize, from: Option<(String, usize)>, depth: usize) -> bool {
    let (mut frame, mut from) = (frame, from);
    for _ in 0..64 {
        if depth > 32 {
            return false;
        }
        let Some((name, out)) = from.clone() else { return false };
        let Some(x) = l.expression(frame, &name) else { return false };
        match short(&x.class) {
            "MakeMaterialAttributes" | "SetMaterialAttributes" | "BlendMaterialAttributes" => return true,
            "GetMaterialAttributes" => return out == 0,
            "Reroute" | "NamedRerouteDeclaration" | "FunctionOutput" => {
                from = x.input_at(0).and_then(|i| i.from.clone())
            }
            "NamedRerouteUsage" => from = x.text("Declaration").map(|d| (d.to_string(), 0)),
            "StaticSwitch" | "StaticSwitchParameter" => {
                let branch = if static_switch_value(l, frame, x) { "True" } else { "False" };
                from = x.input(branch).and_then(|i| i.from.clone());
            }
            "Switch" => match switch_input(l, frame, x) {
                Some(picked) => from = x.input(&picked).and_then(|i| i.from.clone()),
                None => return false,
            },
            "LandscapeLayerSwitch" => {
                from = x.input("LayerUsed").or_else(|| x.input("LayerNotUsed")).and_then(|i| i.from.clone())
            }
            "LandscapeLayerBlend" => {
                return x
                    .inputs
                    .iter()
                    .filter(|i| i.name.starts_with("Layer "))
                    .any(|i| carries_attributes(l, frame, i.from.clone(), depth + 1));
            }
            "Multiply" | "Add" | "Subtract" | "Divide" | "LinearInterpolate" => {
                return ["A", "B"]
                    .iter()
                    .any(|n| carries_attributes(l, frame, x.input(n).and_then(|i| i.from.clone()), depth + 1));
            }
            "MaterialFunctionCall" => {
                let Ok(Some(called)) = l.call_frame(frame, x) else { return false };
                let list = l.function_expressions(called).unwrap_or(&[]);
                let Some(output) = l.call_output(x, list, out) else { return false };
                from = output.input_at(0).and_then(|i| i.from.clone());
                frame = called;
            }
            "FunctionInput" => {
                if x.text("InputType") == Some("FunctionInput_MaterialAttributes") {
                    return true;
                }
                match (l.caller(frame), function_input_position(l, frame, x)) {
                    (Some((caller, call)), Some(position)) => {
                        from = call.input_at(position).and_then(|i| i.from.clone());
                        frame = caller;
                    }
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
    false
}

/// Multiply, Add, Subtract, Divide and LinearInterpolate with material attributes: Unreal compiles attributes one at
/// a time, so the node applies to each attribute, an attributes input giving that attribute (its default when it is
/// not set) and a value input the same value for every attribute. Every attribute with a default is computed.
fn attributes_arithmetic(
    l: &mut Lowerer,
    material: &Material,
    frame: usize,
    x: &Expression,
    depth: usize,
) -> Result<Attributes, Error> {
    let origin = l.origin(frame, x);
    let class = short(&x.class).to_string();
    let names: &[&str] = if class == "LinearInterpolate" { &["A", "B", "Alpha"] } else { &["A", "B"] };
    let mut operands: Vec<Result<Attributes, NodeId>> = Vec::new();
    for name in names {
        let from = x.input(name).and_then(|i| i.from.clone());
        if carries_attributes(l, frame, from.clone(), 0) {
            operands.push(Ok(attributes(l, material, frame, from, depth + 1)?));
        } else {
            let (constant, default) = match *name {
                "A" => ("ConstA", if class == "LinearInterpolate" { 0.0 } else { 0.0 }),
                "B" => (
                    "ConstB",
                    if class == "Multiply" || class == "Divide" || class == "LinearInterpolate" { 1.0 } else { 0.0 },
                ),
                _ => ("ConstAlpha", 0.5),
            };
            operands.push(Err(l.input_or(frame, x, name, constant, default)?));
        }
    }
    let mut out = Attributes::default();
    let set_by_an_operand = |pin: &str| operands.iter().any(|o| matches!(o, Ok(set) if set.has(pin)));
    for (pin, default) in ATTRIBUTE_DEFAULTS.iter() {
        // the pins no surface reads, and the ones that only mean something when set, are left out unless an
        // attributes operand sets them
        if (IGNORED.contains(pin) || *pin == "Anisotropy" || *pin == "Tangent") && !set_by_an_operand(pin) {
            continue;
        }
        let width = default.ty().component_count();
        let mut values = Vec::with_capacity(operands.len());
        for operand in &operands {
            let id = match operand {
                Ok(set) => match set.value(l, pin)? {
                    Some(id) => id,
                    None => l.e.constant(default.clone(), &origin),
                },
                Err(id) => *id,
            };
            values.push(id);
        }
        let value = if class == "LinearInterpolate" {
            let (a, b) = l.widen(values[0], values[1], &origin)?;
            let a = l.cast(a, width, &origin)?;
            let b = l.cast(b, width, &origin)?;
            let alpha = l.cast(values[2], 1, &origin)?;
            let ty = l.e.dag.node(a).ty;
            l.op("mix", ty, &[("fg", Arg::Node(b)), ("bg", Arg::Node(a)), ("mix", Arg::Node(alpha))], &origin)?
        } else {
            let category = match class.as_str() {
                "Multiply" => "multiply",
                "Add" => "add",
                "Subtract" => "subtract",
                _ => "divide",
            };
            let result = l.op2(category, values[0], values[1], &origin)?;
            l.cast(result, width, &origin)?
        };
        out.lowered.insert(pin.to_string(), value);
    }
    Ok(out)
}

/// LandscapeLayerBlend of material attributes: every pin any layer sets is `default + Σ (layer - default) w`, the
/// weights as for values (renormalized when a layer is height-blended), alpha-blended layers lerped in last; a layer
/// that does not set the pin gives the default (so contributes nothing).
fn landscape_blend(
    l: &mut Lowerer,
    material: &Material,
    frame: usize,
    x: &Expression,
    depth: usize,
) -> Result<Attributes, Error> {
    use super::landscape::{blend_layers, blend_values, blend_weights};
    let origin = l.origin(frame, x);
    let layers = blend_layers(x);
    let weights = blend_weights(l, frame, x, &layers)?;
    let mut sets = Vec::with_capacity(layers.len());
    for layer in &layers {
        let from = x.input(&format!("Layer {}", layer.name)).and_then(|i| i.from.clone());
        sets.push(attributes(l, material, frame, from, depth + 1)?);
    }
    let pins: BTreeSet<String> = sets.iter().flat_map(|s| s.links.keys().chain(s.lowered.keys()).cloned()).collect();
    let mut out = Attributes::default();
    for pin in pins {
        let Some((_, default)) = ATTRIBUTE_DEFAULTS.iter().find(|(name, _)| *name == pin) else {
            warn(l, material, format!("{}: {pin} is not blended, the first layer setting it is taken", x.name));
            if let Some(set) = sets.iter().find(|s| s.has(&pin)) {
                if let Some(source) = set.links.get(&pin) {
                    out.links.insert(pin.clone(), source.clone());
                } else if let Some(&id) = set.lowered.get(&pin) {
                    out.lowered.insert(pin.clone(), id);
                }
            }
            continue;
        };
        let width = default.ty().component_count();
        let base = l.e.constant(default.clone(), &origin);
        let base = l.cast(base, width, &origin)?;
        let mut values = Vec::with_capacity(sets.len());
        for set in &sets {
            values.push(match set.value(l, &pin)? {
                Some(id) => Some(l.cast(id, width, &origin)?),
                None => None,
            });
        }
        out.lowered.insert(pin, blend_values(l, &origin, base, &values, &weights, true)?);
    }
    Ok(out)
}

/// BlendMaterialAttributes: every pin either side sets is `lerp(A, B, Alpha)`, the side that leaves it unset giving
/// Unreal's default; `UseA` and `UseB` take one side. Only the pixel blend is read: the vertex attributes it blends
/// (world position offset) are ignored by the surface.
fn blend(
    l: &mut Lowerer,
    material: &Material,
    frame: usize,
    x: &Expression,
    depth: usize,
) -> Result<Attributes, Error> {
    let a = attributes(l, material, frame, x.input("A").and_then(|i| i.from.clone()), depth + 1)?;
    let b = attributes(l, material, frame, x.input("B").and_then(|i| i.from.clone()), depth + 1)?;
    match x.text("PixelAttributeBlendType").unwrap_or("Blend") {
        "UseA" => return Ok(a),
        "UseB" => return Ok(b),
        _ => {}
    }
    let origin = l.origin(frame, x);
    let Some(alpha_input) = x.input("Alpha").filter(|i| i.from.is_some()) else {
        warn(l, material, format!("{}: Alpha is not linked, A is taken", x.name));
        return Ok(a);
    };
    let alpha_from = alpha_input.from.clone().unwrap_or_default();
    let alpha = l.link(frame, &alpha_from, alpha_input.mask)?;
    let alpha = l.cast(alpha, 1, &origin)?;
    let pins: BTreeSet<String> =
        a.links.keys().chain(a.lowered.keys()).chain(b.links.keys()).chain(b.lowered.keys()).cloned().collect();
    let mut out = Attributes::default();
    for pin in pins {
        let Some((_, default)) = ATTRIBUTE_DEFAULTS.iter().find(|(name, _)| *name == pin) else {
            let side = if a.has(&pin) { &a } else { &b };
            warn(l, material, format!("{}: {pin} is not blended, one side's is taken", x.name));
            if let Some(source) = side.links.get(&pin) {
                out.links.insert(pin.clone(), source.clone());
            } else if let Some(&id) = side.lowered.get(&pin) {
                out.lowered.insert(pin.clone(), id);
            }
            continue;
        };
        let width = default.ty().component_count();
        let mut values = [None, None];
        for (k, set) in [&a, &b].into_iter().enumerate() {
            let id = match set.value(l, &pin)? {
                Some(id) => id,
                None => l.e.constant(default.clone(), &origin),
            };
            values[k] = Some(l.cast(id, width, &origin)?);
        }
        let (va, vb) = (values[0].unwrap(), values[1].unwrap());
        let ty = l.e.dag.node(va).ty;
        let mixed =
            l.op("mix", ty, &[("fg", Arg::Node(vb)), ("bg", Arg::Node(va)), ("mix", Arg::Node(alpha))], &origin)?;
        out.lowered.insert(pin, mixed);
    }
    Ok(out)
}

/// The Front Material pin: a Substrate Shading Models node (through reroutes) gives its inputs as the legacy pins
/// and its model; anything else is a Substrate tree.
fn front_material(l: &mut Lowerer, pins: &mut Pins, front: Source) {
    let mut from = Some(front.from.clone());
    for _ in 0..64 {
        let Some((name, _)) = from.clone() else { break };
        let Some(x) = l.expression(0, &name) else { break };
        match short(&x.class) {
            "Reroute" | "NamedRerouteDeclaration" => from = x.input_at(0).and_then(|i| i.from.clone()),
            "SubstrateShadingModels" => {
                for input in &x.inputs {
                    let pin = super::substrate::SHADING_MODELS_PINS.get(input.index).copied().flatten();
                    if let (Some(pin), Some(link)) = (pin, &input.from) {
                        pins.sources.insert(pin.to_string(), Source { frame: 0, from: link.clone(), mask: input.mask });
                    }
                }
                if !pins.sources.contains_key("ShadingModel") {
                    pins.model = x.text("ShadingModelOverride").map(str::to_string);
                }
                return;
            }
            _ => break,
        }
    }
    pins.front = Some(front);
}

/// A tangent-space vector in Unreal's convention as a direction in Shadewire's world: Unreal's tangent frame has
/// its Y along decreasing V, Shadewire's bitangent along increasing V.
fn tangent_to_world(l: &mut Lowerer, v: NodeId, origin: &str) -> Result<NodeId, Error> {
    let v = l.cast(v, 3, origin)?;
    let t = l.e.context(ContextRead::Tangent(Space::World), origin);
    let b = l.e.context(ContextRead::Bitangent(Space::World), origin);
    let n = l.e.context(ContextRead::Normal(Space::World), origin);
    let (x, y, z) = (l.channel(v, 0, origin)?, l.channel(v, 1, origin)?, l.channel(v, 2, origin)?);
    let minus = l.k(-1.0, origin);
    let y = l.op2("multiply", y, minus, origin)?;
    let tx = l.op2("multiply", t, x, origin)?;
    let by = l.op2("multiply", b, y, origin)?;
    let nz = l.op2("multiply", n, z, origin)?;
    let sum = l.op2("add", tx, by, origin)?;
    let sum = l.op2("add", sum, nz, origin)?;
    l.op1("normalize", sum, origin)
}

/// A normal or tangent pin as a Shadewire world direction.
pub(super) fn direction(l: &mut Lowerer, v: NodeId, tangent_space: bool, origin: &str) -> Result<NodeId, Error> {
    if tangent_space {
        tangent_to_world(l, v, origin)
    } else {
        let v = l.cast(v, 3, origin)?;
        let v = l.unreal_direction(v, origin)?;
        l.op1("normalize", v, origin)
    }
}

/// The dag finished with `base`'s surface, and the lowering's warnings.
pub fn finish(mut l: Lowerer, base: &Material) -> Result<(Dag, Vec<String>), Error> {
    let p = pins(&mut l, base)?;
    let settings = l.params.settings.clone();
    if settings.domain != "MD_Surface" {
        warn(&mut l, base, format!("domain {} read as a surface", settings.domain));
    }
    for pin in IGNORED {
        if p.linked(pin) {
            warn(&mut l, base, format!("{pin} ignored"));
        }
    }
    let model = match (&p.model, settings.shading_models.as_slice()) {
        (Some(model), _) => model.clone(),
        (None, []) => "MSM_DefaultLit".to_string(),
        (None, [one]) => one.clone(),
        (None, [first, ..]) => {
            let what = format!("several shading models ({}), {first} used", settings.shading_models.join(", "));
            warn(&mut l, base, what);
            first.clone()
        }
    };
    let mut fields: Vec<(String, Arg)> = Vec::new();

    let opacity = opacity(&mut l, &p, base, &settings.blend_mode, settings.opacity_mask_clip)?;
    if let Some(front) = p.front.clone() {
        return super::substrate::finish(l, base, front.frame, &front.from, opacity);
    }
    if model == "MSM_Unlit" {
        bind(&mut fields, "emission_color", p.color(&mut l, "EmissiveColor", [0.0; 3])?);
        if let Some(opacity) = opacity {
            bind(&mut fields, "opacity", opacity);
        }
        let fields: Vec<(&str, Arg)> = fields.iter().map(|(n, a)| (n.as_str(), a.clone())).collect();
        let Lowerer { e, warnings, .. } = l;
        return Ok((e.finish_model("surface_unlit", &fields, &base.name)?, warnings));
    }

    let origin = |pin: &str| p.origin(pin);
    bind(&mut fields, "base_color", p.color(&mut l, "BaseColor", [0.0; 3])?);
    if p.linked("Metallic") {
        bind(&mut fields, "base_metalness", p.float(&mut l, "Metallic", 0.0)?);
    }
    if p.linked("Specular") {
        let specular = p.float(&mut l, "Specular", 0.5)?;
        let two = l.k(2.0, &origin("Specular"));
        bind(&mut fields, "specular_weight", l.op2("multiply", specular, two, &origin("Specular"))?);
    }
    bind(&mut fields, "specular_roughness", p.float(&mut l, "Roughness", 0.5)?);
    if p.linked("Anisotropy") {
        let anisotropy = p.float(&mut l, "Anisotropy", 0.0)?;
        warn(&mut l, base, "Anisotropy below 0 is clamped to 0 (OpenPBR has no negative anisotropy)");
        bind(
            &mut fields,
            "specular_roughness_anisotropy",
            super::math::clamp01(&mut l, anisotropy, &origin("Anisotropy"))?,
        );
    }
    for (pin, field) in [("Normal", "geometry_normal"), ("Tangent", "geometry_tangent")] {
        if let Some(v) = p.read(&mut l, pin)? {
            bind(&mut fields, field, direction(&mut l, v, settings.tangent_space_normal, &origin(pin))?);
        }
    }
    if p.linked("EmissiveColor") {
        bind(&mut fields, "emission_color", p.color(&mut l, "EmissiveColor", [0.0; 3])?);
        bind(&mut fields, "emission_luminance", l.k(1.0, &origin("EmissiveColor")));
    }
    if let Some(opacity) = opacity {
        bind(&mut fields, "geometry_opacity", opacity);
    }
    if p.linked("Refraction") && settings.blend_mode != "BLEND_Opaque" && settings.blend_mode != "BLEND_Masked" {
        let refraction = p.float(&mut l, "Refraction", 1.0)?;
        warn(&mut l, base, "Refraction read as the specular IOR");
        bind(&mut fields, "specular_ior", refraction);
    }

    match model.as_str() {
        "MSM_DefaultLit" => {}
        "MSM_ClearCoat" => {
            bind(&mut fields, "coat_weight", p.float(&mut l, "CustomData0", 1.0)?);
            bind(&mut fields, "coat_roughness", p.float(&mut l, "CustomData1", 0.1)?);
            bind(&mut fields, "coat_ior", l.k(1.5, &origin("CustomData0")));
        }
        "MSM_Subsurface" | "MSM_PreintegratedSkin" | "MSM_SubsurfaceProfile" => {
            if model != "MSM_Subsurface" {
                warn(&mut l, base, format!("{model} read as MSM_Subsurface"));
            }
            bind(&mut fields, "subsurface_weight", p.float(&mut l, "Opacity", 1.0)?);
            bind(&mut fields, "subsurface_color", p.color(&mut l, "SubsurfaceColor", [1.0; 3])?);
            bind(&mut fields, "subsurface_radius", l.k(0.01, &origin("SubsurfaceColor")));
        }
        "MSM_Cloth" => {
            bind(&mut fields, "fuzz_weight", p.float(&mut l, "CustomData0", 1.0)?);
            bind(&mut fields, "fuzz_color", p.color(&mut l, "SubsurfaceColor", [1.0; 3])?);
        }
        "MSM_TwoSidedFoliage" => {
            warn(&mut l, base, "MSM_TwoSidedFoliage read as thin-walled subsurface");
            bind(&mut fields, "subsurface_weight", l.k(0.5, &origin("SubsurfaceColor")));
            bind(&mut fields, "subsurface_color", p.color(&mut l, "SubsurfaceColor", [1.0; 3])?);
            bind(&mut fields, "geometry_thin_walled", l.e.constant(Value::Boolean(true), &origin("SubsurfaceColor")));
        }
        "MSM_ThinTranslucent" => {
            warn(&mut l, base, "MSM_ThinTranslucent read as thin-walled transmission of the base color");
            bind(&mut fields, "transmission_weight", l.k(1.0, &origin("BaseColor")));
            bind(&mut fields, "transmission_color", p.color(&mut l, "BaseColor", [0.0; 3])?);
            bind(&mut fields, "geometry_thin_walled", l.e.constant(Value::Boolean(true), &origin("BaseColor")));
        }
        other => warn(&mut l, base, format!("{other} read as MSM_DefaultLit")),
    }
    let fields: Vec<(&str, Arg)> = fields.iter().map(|(n, a)| (n.as_str(), a.clone())).collect();
    let Lowerer { e, warnings, .. } = l;
    Ok((e.finish_model("open_pbr_surface", &fields, &base.name)?, warnings))
}

/// The opacity the blend mode makes of the pins: Opacity for translucency, OpacityMask against the clip value for
/// a masked material, none for an opaque one.
fn opacity(l: &mut Lowerer, p: &Pins, base: &Material, blend: &str, clip: f32) -> Result<Option<NodeId>, Error> {
    match blend {
        "BLEND_Opaque" => Ok(None),
        "BLEND_Masked" => {
            let mask = p.float(l, "OpacityMask", 1.0)?;
            let o = p.origin("OpacityMask");
            let (clip, one, zero) = (l.k(clip, &o), l.k(1.0, &o), l.k(0.0, &o));
            let inputs = [
                ("value1", Arg::Node(mask)),
                ("value2", Arg::Node(clip)),
                ("in1", Arg::Node(one)),
                ("in2", Arg::Node(zero)),
            ];
            l.op("ifgreater", Type::Float, &inputs, &o).map(Some)
        }
        "BLEND_Translucent" => p.float(l, "Opacity", 1.0).map(Some),
        other => {
            warn(l, base, format!("{other} read as BLEND_Translucent"));
            p.float(l, "Opacity", 1.0).map(Some)
        }
    }
}
