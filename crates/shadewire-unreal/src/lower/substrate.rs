//! Substrate: the Front Material's tree of BSDFs and operators as a closure tree of pbrlib lobes, and the Substrate
//! helper expressions as the values Unreal computes.
//!
//! A slab is a Schlick specular lobe (F0, F90, roughness) layered over its medium: a diffuse lobe of the diffuse
//! albedo, or a subsurface lobe when the slab has a mean free path. Haze mixes in a second specular lobe, fuzz
//! layers a sheen lobe on top, emission is a uniform EDF. Horizontal mixing is `mix`, vertical layering `layer`,
//! Add `add`, Weight `multiply`, Select a `mix` by a 0 or 1 weight.
//!
//! Substrate nodes are read by input index: their display names change with their settings (a Shading Models
//! node calls its clear coat inputs `Unused` for other models).

use shadewire::{Dag, Error, NodeId, Type, Value};
use shadewire_nodes::document::PortType;
use shadewire_nodes::{Arg, ShaderNode};

use super::surface::{direction, warn};
use super::{Lowerer, short};
use crate::document::{Expression, Material, at};

/// Unreal's default thickness of a Substrate layer, in centimeters.
const DEFAULT_THICKNESS_CM: f32 = 0.01;
/// Unreal's `DielectricSpecularToF0`: F0 per unit of legacy Specular.
const F0_PER_SPECULAR: f32 = 0.08;

/// The Slab BSDF's inputs, in Unreal's order.
mod slab_in {
    pub const DIFFUSE_ALBEDO: usize = 0;
    pub const F0: usize = 1;
    pub const F90: usize = 2;
    pub const ROUGHNESS: usize = 3;
    pub const ANISOTROPY: usize = 4;
    pub const NORMAL: usize = 5;
    pub const TANGENT: usize = 6;
    pub const SSS_MFP: usize = 7;
    pub const SSS_MFP_SCALE: usize = 8;
    pub const SSS_PHASE_ANISOTROPY: usize = 9;
    pub const EMISSIVE: usize = 10;
    pub const SECOND_ROUGHNESS: usize = 11;
    pub const SECOND_ROUGHNESS_WEIGHT: usize = 12;
    pub const FUZZ_ROUGHNESS: usize = 13;
    pub const FUZZ_AMOUNT: usize = 14;
    pub const FUZZ_COLOR: usize = 15;
    pub const GLINT_DENSITY: usize = 16;
    pub const GLINT_UV: usize = 17;
}

/// The Simple Clear Coat BSDF's inputs.
mod clear_coat_in {
    pub const DIFFUSE_ALBEDO: usize = 0;
    pub const F0: usize = 1;
    pub const ROUGHNESS: usize = 2;
    pub const COVERAGE: usize = 3;
    pub const COAT_ROUGHNESS: usize = 4;
    pub const NORMAL: usize = 5;
    pub const EMISSIVE: usize = 6;
    pub const BOTTOM_NORMAL: usize = 7;
}

/// The Unlit BSDF's inputs.
mod unlit_in {
    pub const EMISSIVE: usize = 0;
    pub const TRANSMITTANCE: usize = 1;
}

/// The Substrate Shading Models node's inputs, with the legacy pin each stands for (`None` for the water and
/// thin translucency inputs, which no legacy model here reads).
pub const SHADING_MODELS_PINS: [Option<&str>; 21] = [
    Some("BaseColor"),
    Some("Metallic"),
    Some("Specular"),
    Some("Roughness"),
    Some("Anisotropy"),
    Some("EmissiveColor"),
    Some("Normal"),
    Some("Tangent"),
    Some("SubsurfaceColor"),
    Some("CustomData0"),
    Some("CustomData1"),
    Some("Opacity"),
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    None,
    Some("ShadingModel"),
];

/// A closure tree: its nodes and the names of its BSDF and EDF roots.
#[derive(Clone, Default)]
pub struct Tree {
    pub nodes: Vec<ShaderNode>,
    pub bsdf: Option<String>,
    pub edf: Option<String>,
}

impl Tree {
    /// This tree's nodes then `other`'s, a node both hold once.
    fn absorb(&mut self, other: &Tree) {
        for node in &other.nodes {
            if !self.nodes.iter().any(|n| n.name == node.name) {
                self.nodes.push(node.clone());
            }
        }
    }

    fn set_root(&mut self, kind: &str, name: String) {
        if kind == "BSDF" {
            self.bsdf = Some(name);
        } else {
            self.edf = Some(name);
        }
    }
}

fn lobe(name: String, category: &str, kind: &str, inputs: Vec<(&str, Arg)>) -> ShaderNode {
    ShaderNode {
        name,
        category: category.to_string(),
        port_type: PortType::Closure(kind.to_string()),
        inputs: inputs.into_iter().map(|(n, a)| (n.to_string(), a)).collect(),
        links: Vec::new(),
    }
}

fn operator(
    name: String,
    category: &str,
    kind: &str,
    inputs: Vec<(&str, Arg)>,
    links: Vec<(&str, String)>,
) -> ShaderNode {
    let mut node = lobe(name, category, kind, inputs);
    node.links = links.into_iter().map(|(n, t)| (n.to_string(), t)).collect();
    node
}

/// A `layer` of `top` over `base`.
fn layer(name: String, top: String, base: String) -> ShaderNode {
    operator(name, "layer", "BSDF", Vec::new(), vec![("top", top), ("base", base)])
}

/// Reads a Substrate node's value inputs by index, with Unreal's defaults for the unlinked ones.
struct Reader<'x> {
    f: usize,
    x: &'x Expression,
    origin: String,
}

impl Reader<'_> {
    fn new<'x>(l: &Lowerer, f: usize, x: &'x Expression) -> Reader<'x> {
        Reader { f, x, origin: l.origin(f, x) }
    }

    fn linked(&self, index: usize) -> bool {
        self.x.input_at(index).is_some_and(|i| i.from.is_some())
    }

    fn float(&self, l: &mut Lowerer, index: usize, default: f32) -> Result<NodeId, Error> {
        match l.input_at(self.f, self.x, index)? {
            Some(id) => l.cast(id, 1, &self.origin),
            None => Ok(l.k(default, &self.origin)),
        }
    }

    fn color(&self, l: &mut Lowerer, index: usize, default: f32) -> Result<NodeId, Error> {
        match l.input_at(self.f, self.x, index)? {
            Some(id) => l.color3(id, &self.origin),
            None => Ok(l.e.constant(Value::Color3([default; 3]), &self.origin)),
        }
    }

    /// A normal or tangent input as a Shadewire world direction, when linked.
    fn direction(&self, l: &mut Lowerer, index: usize) -> Result<Option<NodeId>, Error> {
        match l.input_at(self.f, self.x, index)? {
            Some(v) => {
                let tangent_space = l.params.settings.tangent_space_normal;
                direction(l, v, tangent_space, &self.origin).map(Some)
            }
            None => Ok(None),
        }
    }

    /// The input's display name, for messages.
    fn name(&self, index: usize) -> String {
        self.x.input_at(index).map_or_else(|| format!("input {index}"), |i| i.name.clone())
    }
}

/// The GGX roughness pair of a perceptual roughness and an anisotropy.
fn rough2(l: &mut Lowerer, roughness: NodeId, anisotropy: NodeId, origin: &str) -> Result<NodeId, Error> {
    let inputs = [("roughness", Arg::Node(roughness)), ("anisotropy", Arg::Node(anisotropy))];
    l.op("roughness_anisotropy", Type::Vector2, &inputs, origin)
}

/// A specular lobe's parameters.
#[derive(Clone, Copy)]
struct Specular {
    f0: NodeId,
    f90: NodeId,
    roughness: NodeId,
    anisotropy: NodeId,
    normal: Option<NodeId>,
    tangent: Option<NodeId>,
}

/// A Schlick specular lobe called `name`.
fn specular(l: &mut Lowerer, name: String, s: Specular, origin: &str) -> Result<ShaderNode, Error> {
    let roughness = rough2(l, s.roughness, s.anisotropy, origin)?;
    let mut inputs = vec![
        ("color0", Arg::Node(s.f0)),
        ("color90", Arg::Node(s.f90)),
        ("exponent", Arg::Value(Value::Float(5.0))),
        ("roughness", Arg::Node(roughness)),
    ];
    if let Some(n) = s.normal {
        inputs.push(("normal", Arg::Node(n)));
    }
    if let Some(t) = s.tangent {
        inputs.push(("tangent", Arg::Node(t)));
    }
    Ok(lobe(name, "generalized_schlick_bsdf", "BSDF", inputs))
}

/// A diffuse lobe of `albedo`.
fn diffuse(name: String, albedo: NodeId, normal: Option<NodeId>) -> ShaderNode {
    let mut inputs = vec![("color", Arg::Node(albedo))];
    if let Some(n) = normal {
        inputs.push(("normal", Arg::Node(n)));
    }
    lobe(name, "oren_nayar_diffuse_bsdf", "BSDF", inputs)
}

/// The closure tree the BSDF link `from` reads in frame `f`; `top` when it is the top of a vertical layering, where
/// a slab's medium is not read.
pub fn tree(l: &mut Lowerer, f: usize, from: &(String, usize), top: bool) -> Result<Tree, Error> {
    let key = (f, from.0.clone(), from.1, top);
    if let Some(t) = l.trees.get(&key) {
        return Ok(t.clone());
    }
    let Some(x) = l.expression(f, &from.0) else {
        return Err(Error::BadValue { at: at(&from.0), message: format!("no expression named {}", from.0) });
    };
    let t = match short(&x.class) {
        "SubstrateSlabBSDF" => slab(l, f, x, top)?,
        "SubstrateSimpleClearCoatBSDF" => clear_coat(l, f, x, top)?,
        "SubstrateUnlitBSDF" => unlit(l, f, x)?,
        "SubstrateShadingModels" => legacy(l, f, x, top)?,
        "SubstrateHorizontalMixing" => horizontal(l, f, x, top)?,
        "SubstrateVerticalLayering" => vertical(l, f, x, top)?,
        "SubstrateAdd" => add(l, f, x, top)?,
        "SubstrateWeight" => weight(l, f, x, top)?,
        "SubstrateSelect" => select(l, f, x, top)?,
        "Reroute" | "NamedRerouteDeclaration" | "FunctionOutput" => follow(l, f, x, 0, top)?,
        "NamedRerouteUsage" => match x.text("Declaration") {
            Some(d) => tree(l, f, &(d.to_string(), 0), top)?,
            None => Tree::default(),
        },
        "StaticSwitch" | "StaticSwitchParameter" => {
            let branch = if super::params::static_switch_value(l, f, x) { "True" } else { "False" };
            match x.input(branch).and_then(|i| i.from.clone()) {
                Some(from) => tree(l, f, &from, top)?,
                None => Tree::default(),
            }
        }
        "MaterialFunctionCall" => function_call(l, f, x, from.1, top)?,
        "FunctionInput" => function_input(l, f, x, top)?,
        other => {
            l.warn(f, x, format!("{other} is not read, nothing scattered"));
            Tree::default()
        }
    };
    l.trees.insert(key, t.clone());
    Ok(t)
}

/// The tree linked to input `index` of `x`; an empty tree when nothing is.
fn follow(l: &mut Lowerer, f: usize, x: &Expression, index: usize, top: bool) -> Result<Tree, Error> {
    match x.input_at(index).and_then(|i| i.from.clone()) {
        Some(from) => tree(l, f, &from, top),
        None => Ok(Tree::default()),
    }
}

/// The tree linked to input `index` of `x`, an empty tree with a warning when nothing is (Unreal fails to compile).
fn side(l: &mut Lowerer, f: usize, x: &Expression, index: usize, top: bool) -> Result<Tree, Error> {
    match x.input_at(index).and_then(|i| i.from.clone()) {
        Some(from) => tree(l, f, &from, top),
        None => {
            let name = Reader::new(l, f, x).name(index);
            l.warn(f, x, format!("{name} is not linked, nothing scattered there"));
            Ok(Tree::default())
        }
    }
}

fn function_call<'a>(l: &mut Lowerer<'a>, f: usize, x: &'a Expression, out: usize, top: bool) -> Result<Tree, Error> {
    let Some(frame) = l.call_frame(f, x)? else { return Ok(Tree::default()) };
    let list = l.function_expressions(frame).unwrap_or(&[]);
    let Some(output) = l.sorted("FunctionOutput", list).get(out).map(|o| o.name.clone()) else {
        l.warn(f, x, format!("the function has no output {out}, nothing scattered"));
        return Ok(Tree::default());
    };
    tree(l, frame, &(output, 0), top)
}

fn function_input(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let linked = match (l.caller(f), super::params::function_input_position(l, f, x)) {
        (Some((caller, call)), Some(position)) => {
            call.input_at(position).and_then(|i| i.from.clone()).map(|from| (caller, from))
        }
        _ => None,
    };
    match linked {
        Some((caller, from)) => tree(l, caller, &from, top),
        None => follow(l, f, x, 0, top),
    }
}

/// A slab: specular over its medium (left out on top of a vertical layering), haze, fuzz and emission.
fn slab(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    use slab_in::*;
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let s = Specular {
        f0: r.color(l, F0, 0.5 * F0_PER_SPECULAR)?,
        f90: r.color(l, F90, 1.0)?,
        roughness: r.float(l, ROUGHNESS, 0.5)?,
        anisotropy: r.float(l, ANISOTROPY, 0.0)?,
        normal: r.direction(l, NORMAL)?,
        tangent: if r.linked(ANISOTROPY) { r.direction(l, TANGENT)? } else { None },
    };
    if r.linked(GLINT_DENSITY) || r.linked(GLINT_UV) {
        l.warn(f, x, "glints dropped");
    }
    for (property, what) in [("SpecularProfile", "specular profile"), ("SubsurfaceProfile", "subsurface profile")] {
        if x.properties.get(property).is_some_and(|v| !v.is_null()) {
            l.warn(f, x, format!("{what} dropped"));
        }
    }

    let mut t = Tree::default();
    let lobe0 = specular(l, format!("{o}.specular"), s, &o)?;
    let mut root = lobe0.name.clone();
    t.nodes.push(lobe0);
    if r.linked(SECOND_ROUGHNESS_WEIGHT) {
        let second = Specular { roughness: r.float(l, SECOND_ROUGHNESS, 0.0)?, ..s };
        let weight = r.float(l, SECOND_ROUGHNESS_WEIGHT, 0.0)?;
        let hazy = specular(l, format!("{o}.haze"), second, &o)?;
        let mix = operator(
            format!("{o}.specular_mix"),
            "mix",
            "BSDF",
            vec![("mix", Arg::Node(weight))],
            vec![("bg", root), ("fg", hazy.name.clone())],
        );
        root = mix.name.clone();
        t.nodes.push(hazy);
        t.nodes.push(mix);
    }

    if top {
        // A black medium (a clear coat) loses nothing; Unreal's default albedo is 0.18.
        let albedo = r.color(l, DIFFUSE_ALBEDO, 0.18)?;
        let black = l.e.fold_constant(albedo).is_some_and(|v| v.floats().iter().all(|c| *c == 0.0));
        if !black || r.linked(SSS_MFP) {
            l.warn(f, x, "the medium of a top layer is not read: the layer is its specular and fuzz");
        }
    } else {
        let albedo = r.color(l, DIFFUSE_ALBEDO, 0.18)?;
        let medium = if r.linked(SSS_MFP) {
            let mfp = r.color(l, SSS_MFP, 0.0)?;
            let scale = r.float(l, SSS_MFP_SCALE, 1.0)?;
            let anisotropy = r.float(l, SSS_PHASE_ANISOTROPY, 0.0)?;
            // Unreal's centimeters to Shadewire's meters.
            let per_cm = l.k(0.01, &o);
            let meters = l.op2("multiply", scale, per_cm, &o)?;
            let radius = l.op2("multiply", mfp, meters, &o)?;
            let radius = l.color3(radius, &o)?;
            let mut inputs = vec![
                ("color", Arg::Node(albedo)),
                ("radius", Arg::Node(radius)),
                ("anisotropy", Arg::Node(anisotropy)),
            ];
            if let Some(n) = s.normal {
                inputs.push(("normal", Arg::Node(n)));
            }
            lobe(format!("{o}.subsurface"), "subsurface_bsdf", "BSDF", inputs)
        } else {
            diffuse(format!("{o}.diffuse"), albedo, s.normal)
        };
        let slab = layer(format!("{o}.slab"), root, medium.name.clone());
        root = slab.name.clone();
        t.nodes.push(medium);
        t.nodes.push(slab);
    }

    if r.linked(FUZZ_AMOUNT) {
        let amount = r.float(l, FUZZ_AMOUNT, 0.0)?;
        let color = r.color(l, FUZZ_COLOR, 0.0)?;
        let fuzz_roughness = if r.linked(FUZZ_ROUGHNESS) { r.float(l, FUZZ_ROUGHNESS, 0.5)? } else { s.roughness };
        let mut inputs =
            vec![("weight", Arg::Node(amount)), ("color", Arg::Node(color)), ("roughness", Arg::Node(fuzz_roughness))];
        if let Some(n) = s.normal {
            inputs.push(("normal", Arg::Node(n)));
        }
        let sheen = lobe(format!("{o}.fuzz"), "sheen_bsdf", "BSDF", inputs);
        let fuzzed = layer(format!("{o}.fuzzed"), sheen.name.clone(), root);
        root = fuzzed.name.clone();
        t.nodes.push(sheen);
        t.nodes.push(fuzzed);
    }
    t.bsdf = Some(root);
    emission(l, &r, EMISSIVE, &mut t)?;
    Ok(t)
}

/// A uniform EDF of the node's emissive input when it is linked.
fn emission(l: &mut Lowerer, r: &Reader, index: usize, t: &mut Tree) -> Result<(), Error> {
    if !r.linked(index) {
        return Ok(());
    }
    let color = r.color(l, index, 0.0)?;
    let edf = lobe(format!("{}.emission", r.origin), "uniform_edf", "EDF", vec![("color", Arg::Node(color))]);
    t.edf = Some(edf.name.clone());
    t.nodes.push(edf);
    Ok(())
}

/// Simple Clear Coat: a dielectric coat of Clear Coat Coverage over a slab of the other inputs.
fn clear_coat(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    use clear_coat_in::*;
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let coat_normal = r.direction(l, NORMAL)?;
    let base_normal = if r.linked(BOTTOM_NORMAL) { r.direction(l, BOTTOM_NORMAL)? } else { coat_normal };
    let isotropic = l.k(0.0, &o);
    let s = Specular {
        f0: r.color(l, F0, 0.5 * F0_PER_SPECULAR)?,
        f90: l.e.constant(Value::Color3([1.0; 3]), &o),
        roughness: r.float(l, ROUGHNESS, 0.5)?,
        anisotropy: isotropic,
        normal: base_normal,
        tangent: None,
    };
    let mut t = Tree::default();
    let spec = specular(l, format!("{o}.specular"), s, &o)?;
    let mut root = spec.name.clone();
    t.nodes.push(spec);
    if !top {
        let albedo = r.color(l, DIFFUSE_ALBEDO, 0.18)?;
        let medium = diffuse(format!("{o}.diffuse"), albedo, base_normal);
        let slab = layer(format!("{o}.slab"), root, medium.name.clone());
        root = slab.name.clone();
        t.nodes.push(medium);
        t.nodes.push(slab);
    }
    let coverage = r.float(l, COVERAGE, 1.0)?;
    let coat_roughness = r.float(l, COAT_ROUGHNESS, 0.1)?;
    let coat_roughness = rough2(l, coat_roughness, isotropic, &o)?;
    let mut inputs = vec![
        ("weight", Arg::Node(coverage)),
        ("ior", Arg::Value(Value::Float(1.5))),
        ("roughness", Arg::Node(coat_roughness)),
        ("scatter_mode", Arg::Text("R".to_string())),
    ];
    if let Some(n) = coat_normal {
        inputs.push(("normal", Arg::Node(n)));
    }
    let coat = lobe(format!("{o}.coat"), "dielectric_bsdf", "BSDF", inputs);
    let coated = layer(format!("{o}.coated"), coat.name.clone(), root);
    t.bsdf = Some(coated.name.clone());
    t.nodes.push(coat);
    t.nodes.push(coated);
    emission(l, &r, EMISSIVE, &mut t)?;
    Ok(t)
}

/// Unlit: its emission (black when unlinked); the light it lets through is not read.
fn unlit(l: &mut Lowerer, f: usize, x: &Expression) -> Result<Tree, Error> {
    use unlit_in::*;
    let r = Reader::new(l, f, x);
    if r.linked(TRANSMITTANCE) {
        l.warn(f, x, "TransmittanceColor dropped");
    }
    let color = r.color(l, EMISSIVE, 0.0)?;
    let edf = lobe(format!("{}.emission", r.origin), "uniform_edf", "EDF", vec![("color", Arg::Node(color))]);
    Ok(Tree { bsdf: None, edf: Some(edf.name.clone()), nodes: vec![edf] })
}

/// Substrate Shading Models below an operator: the legacy pins as a slab (Unreal's metalness conversion); clear coat,
/// subsurface and the other models' inputs are not read there.
fn legacy(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let pin = |name: &str| SHADING_MODELS_PINS.iter().position(|p| *p == Some(name)).expect("a Shading Models pin");
    let base = r.color(l, pin("BaseColor"), 0.18)?;
    let metallic = r.float(l, pin("Metallic"), 0.0)?;
    let specular_level = r.float(l, pin("Specular"), 0.5)?;
    let (albedo, f0) = metalness(l, base, metallic, specular_level, &o)?;
    for name in ["SubsurfaceColor", "CustomData0", "CustomData1"] {
        if r.linked(pin(name)) {
            l.warn(f, x, format!("{} is not read below a Substrate operator", r.name(pin(name))));
        }
    }
    let s = Specular {
        f0,
        f90: l.e.constant(Value::Color3([1.0; 3]), &o),
        roughness: r.float(l, pin("Roughness"), 0.5)?,
        anisotropy: r.float(l, pin("Anisotropy"), 0.0)?,
        normal: r.direction(l, pin("Normal"))?,
        tangent: if r.linked(pin("Anisotropy")) { r.direction(l, pin("Tangent"))? } else { None },
    };
    let mut t = Tree::default();
    let spec = specular(l, format!("{o}.specular"), s, &o)?;
    let mut root = spec.name.clone();
    t.nodes.push(spec);
    if !top {
        let medium = diffuse(format!("{o}.diffuse"), albedo, s.normal);
        let slab = layer(format!("{o}.slab"), root, medium.name.clone());
        root = slab.name.clone();
        t.nodes.push(medium);
        t.nodes.push(slab);
    }
    t.bsdf = Some(root);
    emission(l, &r, pin("EmissiveColor"), &mut t)?;
    Ok(t)
}

/// `(DiffuseAlbedo, F0)` of a metalness parameterization: `BaseColor·(1 − Metallic)` and
/// `lerp(0.08·Specular, BaseColor, Metallic)`, Unreal's `ComputeDiffuseAlbedo` and `ComputeF0`.
fn metalness(
    l: &mut Lowerer,
    base: NodeId,
    metallic: NodeId,
    specular: NodeId,
    o: &str,
) -> Result<(NodeId, NodeId), Error> {
    let base = l.color3(base, o)?;
    let one = l.k(1.0, o);
    let rest = l.op2("subtract", one, metallic, o)?;
    let rest = l.color3(rest, o)?;
    let albedo = l.op2("multiply", base, rest, o)?;
    let albedo = l.color3(albedo, o)?;
    let per = l.k(F0_PER_SPECULAR, o);
    let dielectric = l.op2("multiply", specular, per, o)?;
    let dielectric = l.color3(dielectric, o)?;
    let inputs = [("fg", Arg::Node(base)), ("bg", Arg::Node(dielectric)), ("mix", Arg::Node(metallic))];
    let f0 = l.op("mix", Type::Color3, &inputs, o)?;
    Ok((albedo, f0))
}

/// The roots of two trees combined: mixed by `mix` (`a` the background, `b` the foreground) or added; a root only
/// one side has is weighted by that side's share of the mix.
fn combine(l: &mut Lowerer, o: &str, a: Tree, b: Tree, mix: Option<NodeId>) -> Result<Tree, Error> {
    let mut t = Tree::default();
    t.absorb(&a);
    t.absorb(&b);
    for (kind, ra, rb) in [("BSDF", a.bsdf.clone(), b.bsdf.clone()), ("EDF", a.edf.clone(), b.edf.clone())] {
        let name = format!("{o}.{}", kind.to_lowercase());
        let node = match (ra, rb, mix) {
            (None, None, _) => continue,
            (Some(a), Some(b), Some(m)) => {
                operator(name, "mix", kind, vec![("mix", Arg::Node(m))], vec![("bg", a), ("fg", b)])
            }
            (Some(a), Some(b), None) => operator(name, "add", kind, Vec::new(), vec![("in1", a), ("in2", b)]),
            (Some(only), None, None) | (None, Some(only), None) => {
                t.set_root(kind, only);
                continue;
            }
            (Some(only), None, Some(m)) => {
                let one = l.k(1.0, o);
                let w = l.op2("subtract", one, m, o)?;
                let w = l.color3(w, o)?;
                operator(name, "multiply", kind, vec![("in2", Arg::Node(w))], vec![("in1", only)])
            }
            (None, Some(only), Some(m)) => {
                let w = l.color3(m, o)?;
                operator(name, "multiply", kind, vec![("in2", Arg::Node(w))], vec![("in1", only)])
            }
        };
        t.set_root(kind, node.name.clone());
        t.nodes.push(node);
    }
    Ok(t)
}

/// Horizontal mixing: Background (0) and Foreground (1) mixed by Mix (2).
fn horizontal(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let r = Reader::new(l, f, x);
    if x.flag("bUseParameterBlending") {
        l.warn(f, x, "parameter blending read as a mix of the two sides");
    }
    let background = side(l, f, x, 0, top)?;
    let foreground = side(l, f, x, 1, top)?;
    let mix = r.float(l, 2, 0.5)?;
    combine(l, &r.origin, background, foreground, Some(mix))
}

/// Vertical layering: the top (0)'s specular and fuzz layered over the bottom (1); the emission of both added.
fn vertical(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let o = l.origin(f, x);
    if x.input_at(2).is_some_and(|i| i.from.is_some()) {
        l.warn(f, x, format!("Top Thickness read as the default {DEFAULT_THICKNESS_CM} cm"));
    }
    let upper = side(l, f, x, 0, true)?;
    let lower = side(l, f, x, 1, top)?;
    let mut t = Tree::default();
    t.absorb(&upper);
    t.absorb(&lower);
    t.bsdf = match (upper.bsdf.clone(), lower.bsdf.clone()) {
        (Some(a), Some(b)) => {
            let node = layer(format!("{o}.bsdf"), a, b);
            let name = node.name.clone();
            t.nodes.push(node);
            Some(name)
        }
        (a, b) => a.or(b),
    };
    let upper_edf = Tree { nodes: Vec::new(), bsdf: None, edf: upper.edf.clone() };
    let lower_edf = Tree { nodes: Vec::new(), bsdf: None, edf: lower.edf.clone() };
    let emitted = combine(l, &o, upper_edf, lower_edf, None)?;
    t.absorb(&emitted);
    t.edf = emitted.edf;
    Ok(t)
}

fn add(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let o = l.origin(f, x);
    let a = side(l, f, x, 0, top)?;
    let b = side(l, f, x, 1, top)?;
    combine(l, &o, a, b, None)
}

/// Weight: A (0) times Weight (1).
fn weight(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let a = side(l, f, x, 0, top)?;
    let w = r.float(l, 1, 1.0)?;
    let w = l.color3(w, &o)?;
    let mut t = Tree::default();
    t.absorb(&a);
    for (kind, root) in [("BSDF", a.bsdf.clone()), ("EDF", a.edf.clone())] {
        let Some(root) = root else { continue };
        let name = format!("{o}.{}", kind.to_lowercase());
        let node = operator(name, "multiply", kind, vec![("in2", Arg::Node(w))], vec![("in1", root)]);
        t.set_root(kind, node.name.clone());
        t.nodes.push(node);
    }
    Ok(t)
}

/// Select: A (0) where SelectValue (2) is above Threshold, else B (1).
fn select(l: &mut Lowerer, f: usize, x: &Expression, top: bool) -> Result<Tree, Error> {
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let a = side(l, f, x, 0, top)?;
    let b = side(l, f, x, 1, top)?;
    let value = r.float(l, 2, 0.0)?;
    let threshold = l.k(x.number_or("Threshold", 0.5), &o);
    let (one, zero) = (l.k(1.0, &o), l.k(0.0, &o));
    let inputs = [
        ("value1", Arg::Node(value)),
        ("value2", Arg::Node(threshold)),
        ("in1", Arg::Node(one)),
        ("in2", Arg::Node(zero)),
    ];
    let pick_a = l.op("ifgreater", Type::Float, &inputs, &o)?;
    combine(l, &o, b, a, Some(pick_a))
}

/// The dag finished as the closure tree the Front Material reads, with `opacity` from the blend mode.
pub fn finish(
    mut l: Lowerer,
    base: &Material,
    frame: usize,
    from: &(String, usize),
    opacity: Option<NodeId>,
) -> Result<(Dag, Vec<String>), Error> {
    if l.params.settings.blend_mode == "BLEND_Translucent" {
        warn(&mut l, base, "Substrate translucency is not read, the material is opaque");
    }
    let t = tree(&mut l, frame, from, false)?;
    let mut nodes = t.nodes.clone();
    let mut name = "surface".to_string();
    while nodes.iter().any(|n| n.name == name) {
        name.push('_');
    }
    let mut links = Vec::new();
    if let Some(bsdf) = t.bsdf {
        links.push(("bsdf".to_string(), bsdf));
    }
    if let Some(edf) = t.edf {
        links.push(("edf".to_string(), edf));
    }
    let mut inputs = Vec::new();
    if let Some(opacity) = opacity.filter(|_| l.params.settings.blend_mode == "BLEND_Masked") {
        inputs.push(("opacity".to_string(), Arg::Node(opacity)));
    }
    nodes.push(ShaderNode {
        name: name.clone(),
        category: "surface".to_string(),
        port_type: PortType::SurfaceShader,
        inputs,
        links,
    });
    let Lowerer { e, warnings, .. } = l;
    Ok((e.finish_closure(nodes, &name)?, warnings))
}

/// Metalness-To-DiffuseAlbedo-F0 (BaseColor 0, Metallic 1, Specular 2): output 0 the diffuse albedo, 1 the F0.
pub fn metalness_to_albedo_f0(l: &mut Lowerer, f: usize, x: &Expression, out: usize) -> Result<NodeId, Error> {
    let r = Reader::new(l, f, x);
    let base = r.color(l, 0, 0.18)?;
    let metallic = r.float(l, 1, 0.0)?;
    let specular = r.float(l, 2, 0.5)?;
    let (albedo, f0) = metalness(l, base, metallic, specular, &r.origin)?;
    l.cast(if out == 0 { albedo } else { f0 }, 3, &r.origin)
}

/// Haziness-To-Secondary-Roughness (BaseRoughness 0, Haziness 1): Unreal's `SubstrateComputeHazeRoughnessAndWeight`
/// ("A Composite BRDF Model for Hazy Gloss"); output 0 the second roughness, 1 its weight.
pub fn haziness(l: &mut Lowerer, f: usize, x: &Expression, out: usize) -> Result<NodeId, Error> {
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let base = r.float(l, 0, 0.1)?;
    let r0 = super::math::clamp01(l, base, &o)?;
    let haze = r.float(l, 1, 0.5)?;
    let haze = super::math::clamp01(l, haze, &o)?;
    let (one, two, four) = (l.k(1.0, &o), l.k(2.0, &o), l.k(4.0, &o));
    let s2 = l.op2("multiply", r0, two, &o)?;
    let s2 = super::math::clamp01(l, s2, &o)?;
    let s4 = l.op2("multiply", r0, four, &o)?;
    let s4 = super::math::clamp01(l, s4, &o)?;
    let extent = lerp(l, 27.0, 3.0, s4, &o)?;
    let core = lerp(l, 0.01, 0.5, s2, &o)?;
    let alpha_n = l.op2("multiply", r0, r0, &o)?;
    let spread = l.op2("add", one, extent, &o)?;
    let alpha_w = l.op2("multiply", alpha_n, spread, &o)?;
    if out == 0 {
        return l.op1("sqrt", alpha_w, &o);
    }
    let spread2 = l.op2("multiply", spread, spread, &o)?;
    let p = l.op2("divide", one, spread2, &o)?;
    // b = 2 (r_c (1 − w) + w p)
    let one_minus_w = l.op2("subtract", one, alpha_w, &o)?;
    let a1 = l.op2("multiply", core, one_minus_w, &o)?;
    let a2 = l.op2("multiply", alpha_w, p, &o)?;
    let sum = l.op2("add", a1, a2, &o)?;
    let b = l.op2("multiply", sum, two, &o)?;
    // D = b² − 4 (b − 1) r_c
    let b2 = l.op2("multiply", b, b, &o)?;
    let b_minus = l.op2("subtract", b, one, &o)?;
    let four_b = l.op2("multiply", b_minus, four, &o)?;
    let four_b = l.op2("multiply", four_b, core, &o)?;
    let d = l.op2("subtract", b2, four_b, &o)?;
    // u = (b − √D) / (2b − 2)
    let root_d = l.op1("sqrt", d, &o)?;
    let num = l.op2("subtract", b, root_d, &o)?;
    let den = l.op2("multiply", b_minus, two, &o)?;
    let u = l.op2("divide", num, den, &o)?;
    let u2 = l.op2("multiply", u, u, &o)?;
    let w2 = l.op2("multiply", alpha_w, two, &o)?;
    // k_h = ((u − u²) w2 β) / ((1 − u)² + (u − u²) w2 + u²)
    let u_minus = l.op2("subtract", u, u2, &o)?;
    let uw = l.op2("multiply", u_minus, w2, &o)?;
    let above = l.op2("multiply", uw, haze, &o)?;
    let one_minus_u = l.op2("subtract", one, u, &o)?;
    let q = l.op2("multiply", one_minus_u, one_minus_u, &o)?;
    let below = l.op2("add", q, uw, &o)?;
    let below = l.op2("add", below, u2, &o)?;
    let k_h = l.op2("divide", above, below, &o)?;
    // r = r_c + (1 − p) k_h; the weight is k_h / r
    let one_minus_p = l.op2("subtract", one, p, &o)?;
    let kp = l.op2("multiply", one_minus_p, k_h, &o)?;
    let rr = l.op2("add", core, kp, &o)?;
    l.op2("divide", k_h, rr, &o)
}

/// `lerp(a, b, t)` of two constants.
fn lerp(l: &mut Lowerer, a: f32, b: f32, t: NodeId, o: &str) -> Result<NodeId, Error> {
    let (a, b) = (l.k(a, o), l.k(b, o));
    l.op("mix", Type::Float, &[("bg", Arg::Node(a)), ("fg", Arg::Node(b)), ("mix", Arg::Node(t))], o)
}

/// Transmittance-To-MeanFreePath (TransmittanceColor 0, Thickness 1): output 0 the mean free path in centimeters,
/// `1 / (−ln T / thickness)` as Unreal's `TransmittanceToMeanFreePath`; output 1 the thickness.
pub fn transmittance_to_mfp(l: &mut Lowerer, f: usize, x: &Expression, out: usize) -> Result<NodeId, Error> {
    let r = Reader::new(l, f, x);
    let o = r.origin.clone();
    let thickness = r.float(l, 1, DEFAULT_THICKNESS_CM)?;
    if out == 1 {
        return Ok(thickness);
    }
    let transmittance = match l.input_at(f, x, 0)? {
        Some(id) => l.cast(id, 3, &o)?,
        None => l.kv(&[0.5; 3], &o),
    };
    let (tiny, one) = (l.k(1e-12, &o), l.k(1.0, &o));
    let t = l.op2("max", transmittance, tiny, &o)?;
    let t = l.op2("min", t, one, &o)?;
    let ln = l.op1("ln", t, &o)?;
    let minus = l.k(-1.0, &o);
    let neg = l.op2("multiply", ln, minus, &o)?;
    let per_cm = l.k(0.01, &o);
    let meters = l.op2("multiply", thickness, per_cm, &o)?;
    let meters = l.op2("max", meters, tiny, &o)?;
    let extinction = l.op2("divide", neg, meters, &o)?;
    let extinction = l.op2("max", extinction, tiny, &o)?;
    let mfp = l.op2("divide", one, extinction, &o)?;
    let cm = l.k(100.0, &o);
    l.op2("multiply", mfp, cm, &o)
}

/// Thin Film (Normal 0, F0 1, F90 2, Thickness 3, IOR 4): its interference is not computed; F0 and F90 pass
/// through.
pub fn thin_film(l: &mut Lowerer, f: usize, x: &Expression, out: usize) -> Result<NodeId, Error> {
    let r = Reader::new(l, f, x);
    l.warn(f, x, "thin-film interference dropped, F0 and F90 pass through");
    let picked = if out == 0 { r.color(l, 1, 0.04)? } else { r.color(l, 2, 1.0)? };
    l.cast(picked, 3, &r.origin)
}
