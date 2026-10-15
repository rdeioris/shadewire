//! Shader sockets: what the Material Output's Surface reads, and the dag finished from it.

use shadewire::{Dag, Error, NodeId, Type};
use shadewire_nodes::document::PortType;
use shadewire_nodes::{Arg, ShaderNode};

use super::{Lowerer, lobes, principled};
use crate::document::at;
use crate::graph::FlatNode;

/// What a shader socket carries.
#[derive(Clone)]
pub enum Shading {
    /// A Principled BSDF: `open_pbr_surface` over these fields.
    Principled { fields: Vec<(String, Arg)>, origin: String },
    /// A tree of lobes and operators.
    Closure(Tree),
    /// Nothing is linked.
    Nothing,
}

/// A closure tree: its shader-level nodes, and the names of its BSDF and EDF roots, with the transparency mixed
/// over them.
#[derive(Clone)]
pub struct Tree {
    pub nodes: Vec<ShaderNode>,
    pub bsdf: Option<String>,
    pub edf: Option<String>,
    /// How much of the light passes through, as a float.
    pub transparency: Option<NodeId>,
}

impl Tree {
    /// A tree of one node and no transparency.
    fn lobe(shader: ShaderNode) -> Tree {
        let (bsdf, edf) = match &shader.port_type {
            PortType::Closure(kind) if kind == "EDF" => (None, Some(shader.name.clone())),
            _ => (Some(shader.name.clone()), None),
        };
        Tree { nodes: vec![shader], bsdf, edf, transparency: None }
    }

    /// A tree that only lets light through.
    fn is_transparency_only(&self) -> bool {
        self.bsdf.is_none() && self.edf.is_none()
    }

    /// This tree's nodes then `other`'s, a node both hold (a shader reached twice) once.
    fn nodes_with(mut self, other: Vec<ShaderNode>) -> Vec<ShaderNode> {
        for node in other {
            if !self.nodes.iter().any(|n| n.name == node.name) {
                self.nodes.push(node);
            }
        }
        self.nodes
    }
}

/// The shading `node`'s shader output carries (the output's name matters to no shader this version lowers), lowered
/// once per node.
pub fn shading(l: &mut Lowerer, node: &FlatNode, _output: &str) -> Result<Shading, Error> {
    if let Some(done) = l.shaded.get(&node.name) {
        return Ok(done.clone());
    }
    if !l.shading_in_progress.insert(node.name.clone()) {
        return Err(Error::Cycle { at: at(&node.name), node: node.name.clone() });
    }
    let shading = shade(l, node);
    l.shading_in_progress.remove(&node.name);
    let shading = shading?;
    l.shaded.insert(node.name.clone(), shading.clone());
    Ok(shading)
}

fn shade(l: &mut Lowerer, node: &FlatNode) -> Result<Shading, Error> {
    match node.kind.as_str() {
        "ShaderNodeBsdfPrincipled" => principled::fields(l, node),
        "ShaderNodeEmission" => emission(l, node),
        "ShaderNodeBsdfTransparent" => {
            let transparency = l.input(node, "Color", Type::Float)?;
            Ok(Shading::Closure(Tree { nodes: Vec::new(), bsdf: None, edf: None, transparency: Some(transparency) }))
        }
        "ShaderNodeMixShader" | "ShaderNodeAddShader" => {
            let a = shader_input(l, node, "Shader")?;
            let b = shader_input(l, node, "Shader_001")?;
            combine(l, node, a, b)
        }
        _ => match lobes::lobe(l, node)? {
            Some(shader) => Ok(Shading::Closure(Tree::lobe(shader))),
            None => {
                l.warn(node, "not a surface shader, nothing scattered");
                Ok(Shading::Nothing)
            }
        },
    }
}

/// What is linked to the shader input `socket` of `node`; nothing when it is not linked.
fn shader_input(l: &mut Lowerer, node: &FlatNode, socket: &str) -> Result<Shading, Error> {
    let graph = l.graph;
    let Some((from, output)) = graph.source(&node.name, socket) else { return Ok(Shading::Nothing) };
    let Some(source) = graph.node(from) else {
        return Err(Error::BadValue { at: at(from), message: format!("no node named {from}") });
    };
    shading(l, source, output)
}

/// Emission: a `uniform_edf` of Color times Strength.
fn emission(l: &mut Lowerer, node: &FlatNode) -> Result<Shading, Error> {
    let color = l.input(node, "Color", Type::Color3)?;
    let strength = l.input(node, "Strength", Type::Float)?;
    let origin = format!("{}.color", node.name);
    let color = l.op("multiply", Type::Color3, &[("in1", Arg::Node(color)), ("in2", Arg::Node(strength))], &origin)?;
    let shader = ShaderNode {
        name: node.name.clone(),
        category: "uniform_edf".to_string(),
        port_type: PortType::Closure("EDF".to_string()),
        inputs: vec![("color".to_string(), Arg::Node(color))],
        links: Vec::new(),
    };
    Ok(Shading::Closure(Tree::lobe(shader)))
}

/// Whether `combine` reads the Fac socket of a Mix Shader over these two sides.
fn needs_fac(a: &Shading, b: &Shading) -> bool {
    match (a, b) {
        (Shading::Closure(_), Shading::Closure(_) | Shading::Nothing) => true,
        (Shading::Nothing, Shading::Closure(_)) => true,
        (Shading::Principled { .. }, Shading::Closure(t)) | (Shading::Closure(t), Shading::Principled { .. }) => {
            t.is_transparency_only()
        }
        _ => false,
    }
}

/// The Mix Shader or Add Shader `node` over `a` (its first shader) and `b` (its second).
fn combine(l: &mut Lowerer, node: &FlatNode, a: Shading, b: Shading) -> Result<Shading, Error> {
    let mix = node.kind == "ShaderNodeMixShader";
    let fac = if mix && needs_fac(&a, &b) { Some(l.input(node, "Fac", Type::Float)?) } else { None };
    match (a, b) {
        (Shading::Nothing, Shading::Nothing) => Ok(Shading::Nothing),
        (a @ Shading::Principled { .. }, Shading::Principled { .. }) => {
            l.warn(node, "two Principled BSDFs, the first wins");
            Ok(a)
        }
        (p @ Shading::Principled { .. }, Shading::Closure(t)) => principled_with(l, node, p, t, fac, true),
        (Shading::Closure(t), p @ Shading::Principled { .. }) => principled_with(l, node, p, t, fac, false),
        (p @ Shading::Principled { .. }, Shading::Nothing) | (Shading::Nothing, p @ Shading::Principled { .. }) => {
            if mix {
                l.warn(node, "one side unlinked");
            }
            Ok(p)
        }
        (Shading::Closure(a), Shading::Closure(b)) => Ok(Shading::Closure(merged(l, node, a, b, fac)?)),
        (Shading::Closure(t), Shading::Nothing) => one_sided(l, node, t, fac, false),
        (Shading::Nothing, Shading::Closure(t)) => one_sided(l, node, t, fac, true),
    }
}

/// A Principled BSDF `p` and a closure tree `t` (the second shader when `t_is_second`): a transparent tree scales
/// the Principled BSDF's opacity, anything else loses to it.
fn principled_with(
    l: &mut Lowerer,
    node: &FlatNode,
    p: Shading,
    t: Tree,
    fac: Option<NodeId>,
    t_is_second: bool,
) -> Result<Shading, Error> {
    if !t.is_transparency_only() {
        l.warn(node, "Principled BSDF mixed with lobes, the Principled BSDF wins");
        return Ok(p);
    }
    let (Shading::Principled { mut fields, origin }, Some(transparency)) = (p.clone(), t.transparency) else {
        return Ok(p);
    };
    let origin_of = |what: &str| format!("{}.{what}", node.name);
    let passed = match fac {
        Some(fac) => {
            let weight = if t_is_second { fac } else { one_minus(l, fac, &origin_of("weight"))? };
            multiply(l, transparency, weight, &origin_of("transparency"))?
        }
        None => transparency,
    };
    let remains = one_minus(l, passed, &origin_of("opacity"))?;
    let opacity = match fields.iter().position(|(name, _)| name == "geometry_opacity") {
        Some(index) => {
            let current = match &fields[index].1 {
                Arg::Node(id) => *id,
                Arg::Value(value) => l.e.constant(value.clone(), &origin_of("opacity")),
                _ => return Ok(Shading::Principled { fields, origin }),
            };
            let opacity = multiply(l, current, remains, &origin_of("opacity"))?;
            fields.remove(index);
            opacity
        }
        None => remains,
    };
    fields.push(("geometry_opacity".to_string(), Arg::Node(opacity)));
    Ok(Shading::Principled { fields, origin })
}

/// Two closure trees combined: the nodes of both, their BSDFs and EDFs mixed or added, their transparency mixed or
/// added.
fn merged(l: &mut Lowerer, node: &FlatNode, a: Tree, b: Tree, fac: Option<NodeId>) -> Result<Tree, Error> {
    let (bsdf_a, bsdf_b, edf_a, edf_b) = (a.bsdf.clone(), b.bsdf.clone(), a.edf.clone(), b.edf.clone());
    let (t_a, t_b) = (a.transparency, b.transparency);
    // Light that passes a transparent side reaches what is behind the surface, not the other side's lobes: they
    // keep their strength and the opacity carries the mix. Only a side that renders black takes energy with it.
    let lobe_fac = if a.is_transparency_only() || b.is_transparency_only() { None } else { fac };
    let mut extra = Vec::new();
    let bsdf = root(l, node, Root::BSDF, [bsdf_a, bsdf_b], lobe_fac, &mut extra)?;
    let edf = root(l, node, Root::EDF, [edf_a, edf_b], lobe_fac, &mut extra)?;
    let origin = format!("{}.transparency", node.name);
    let transparency = match fac {
        Some(fac) => mixed_transparency(l, [t_a, t_b], fac, &origin)?,
        None => added_transparency(l, [t_a, t_b], &origin)?,
    };
    let nodes = a.nodes_with(b.nodes);
    let mut tree = Tree { nodes, bsdf, edf, transparency };
    tree.nodes.extend(extra);
    Ok(tree)
}

/// The transparency of two sides mixed by `fac`; a side with none passes nothing.
fn mixed_transparency(
    l: &mut Lowerer,
    sides: [Option<NodeId>; 2],
    fac: NodeId,
    origin: &str,
) -> Result<Option<NodeId>, Error> {
    if sides == [None, None] {
        return Ok(None);
    }
    let [bg, fg] = sides.map(|t| t.unwrap_or_else(|| l.k(0.0, origin)));
    let inputs = [("bg", Arg::Node(bg)), ("fg", Arg::Node(fg)), ("mix", Arg::Node(fac))];
    l.op("mix", Type::Float, &inputs, origin).map(Some)
}

/// The transparency of two sides added, at most 1.
fn added_transparency(l: &mut Lowerer, sides: [Option<NodeId>; 2], origin: &str) -> Result<Option<NodeId>, Error> {
    let [Some(a), Some(b)] = sides else { return Ok(sides[0].or(sides[1])) };
    let sum = l.op("add", Type::Float, &[("in1", Arg::Node(a)), ("in2", Arg::Node(b))], origin)?;
    let one = l.k(1.0, origin);
    l.op("min", Type::Float, &[("in1", Arg::Node(sum)), ("in2", Arg::Node(one))], origin).map(Some)
}

/// Which root of a closure tree.
#[derive(Clone, Copy)]
struct Root {
    /// The pbrlib closure type.
    kind: &'static str,
    /// The suffix of the combining node's name.
    suffix: &'static str,
}

impl Root {
    const BSDF: Root = Root { kind: "BSDF", suffix: "bsdf" };
    const EDF: Root = Root { kind: "EDF", suffix: "edf" };
}

/// The node `name` of `which` closure type: `category` over value `inputs` and the closures `links` point at.
fn operator(
    name: &str,
    which: Root,
    category: &str,
    inputs: Vec<(&str, Arg)>,
    links: Vec<(&str, String)>,
) -> ShaderNode {
    ShaderNode {
        name: name.to_string(),
        category: category.to_string(),
        port_type: PortType::Closure(which.kind.to_string()),
        inputs: inputs.into_iter().map(|(input, arg)| (input.to_string(), arg)).collect(),
        links: links.into_iter().map(|(input, target)| (input.to_string(), target)).collect(),
    }
}

/// The root of `which` over the two sides' roots, pushing the node that makes it onto `nodes`: a mix of both, a
/// sum, or the side that has one (weighted by its Mix Shader weight; `fac` is `None` for an Add Shader).
fn root(
    l: &mut Lowerer,
    node: &FlatNode,
    which: Root,
    sides: [Option<String>; 2],
    fac: Option<NodeId>,
    nodes: &mut Vec<ShaderNode>,
) -> Result<Option<String>, Error> {
    let name = format!("{}.{}", node.name, which.suffix);
    let shader = match (sides, fac) {
        ([None, None], _) => return Ok(None),
        ([Some(a), Some(b)], Some(fac)) => {
            operator(&name, which, "mix", vec![("mix", Arg::Node(fac))], vec![("bg", a), ("fg", b)])
        }
        ([Some(a), Some(b)], None) => operator(&name, which, "add", Vec::new(), vec![("in1", a), ("in2", b)]),
        ([Some(only), None], None) | ([None, Some(only)], None) => return Ok(Some(only)),
        ([Some(only), None], Some(fac)) => weighted_root(l, node, &name, which, only, fac, false)?,
        ([None, Some(only)], Some(fac)) => weighted_root(l, node, &name, which, only, fac, true)?,
    };
    nodes.push(shader);
    Ok(Some(name))
}

/// A `multiply` of the root `only` by the weight of its side.
fn weighted_root(
    l: &mut Lowerer,
    node: &FlatNode,
    name: &str,
    which: Root,
    only: String,
    fac: NodeId,
    second: bool,
) -> Result<ShaderNode, Error> {
    let origin = format!("{}.weight", node.name);
    let weight = side_weight(l, fac, second, &origin)?;
    let color = l.convert(weight, Type::Color3, &origin)?;
    Ok(operator(name, which, "multiply", vec![("in2", Arg::Node(color))], vec![("in1", only)]))
}

/// A closure tree `t` over nothing: weighted by its side's Mix Shader weight (`fac` is `None` for an Add Shader,
/// which leaves it as it is).
fn one_sided(l: &mut Lowerer, node: &FlatNode, t: Tree, fac: Option<NodeId>, second: bool) -> Result<Shading, Error> {
    let Some(fac) = fac else { return Ok(Shading::Closure(t)) };
    let mut nodes = Vec::new();
    let sides = |root: &Option<String>| if second { [None, root.clone()] } else { [root.clone(), None] };
    let bsdf = root(l, node, Root::BSDF, sides(&t.bsdf), Some(fac), &mut nodes)?;
    let edf = root(l, node, Root::EDF, sides(&t.edf), Some(fac), &mut nodes)?;
    let transparency = match t.transparency {
        Some(passed) => {
            let origin = format!("{}.transparency", node.name);
            let weight = side_weight(l, fac, second, &origin)?;
            Some(multiply(l, passed, weight, &origin)?)
        }
        None => None,
    };
    let mut tree = Tree { nodes: t.nodes, bsdf, edf, transparency };
    tree.nodes.extend(nodes);
    Ok(Shading::Closure(tree))
}

/// The weight Mix Shader gives a side: `fac` for the second shader, `1 - fac` for the first.
fn side_weight(l: &mut Lowerer, fac: NodeId, second: bool, origin: &str) -> Result<NodeId, Error> {
    if second { Ok(fac) } else { one_minus(l, fac, origin) }
}

fn one_minus(l: &mut Lowerer, x: NodeId, origin: &str) -> Result<NodeId, Error> {
    let one = l.k(1.0, origin);
    l.op("subtract", Type::Float, &[("in1", Arg::Node(one)), ("in2", Arg::Node(x))], origin)
}

fn multiply(l: &mut Lowerer, a: NodeId, b: NodeId, origin: &str) -> Result<NodeId, Error> {
    l.op("multiply", Type::Float, &[("in1", Arg::Node(a)), ("in2", Arg::Node(b))], origin)
}

/// The dag finished with `shading` as the surface of `output_node` (`linked`: the node its Surface is linked to), and
/// the lowering's warnings.
pub fn finish(
    mut l: Lowerer,
    shading: Shading,
    output_node: &FlatNode,
    linked: Option<&FlatNode>,
) -> Result<(Dag, Vec<String>), Error> {
    match shading {
        Shading::Principled { fields, origin } => {
            let fields: Vec<(&str, Arg)> = fields.iter().map(|(name, arg)| (name.as_str(), arg.clone())).collect();
            let Lowerer { e, warnings, .. } = l;
            Ok((e.finish_model("open_pbr_surface", &fields, &origin)?, warnings))
        }
        Shading::Closure(tree) => {
            let (nodes, name) = closure_nodes(&mut l, tree)?;
            let Lowerer { e, warnings, .. } = l;
            Ok((e.finish_closure(nodes, &name)?, warnings))
        }
        Shading::Nothing => Err(match linked {
            Some(node) => Error::UnsupportedSurface {
                at: at(&node.name),
                node: node.name.clone(),
                reason: format!("{} is not a surface shader", node.kind),
            },
            None => Error::UnsupportedSurface {
                at: at(&output_node.name),
                node: output_node.name.clone(),
                reason: "Surface is not linked".to_string(),
            },
        }),
    }
}

/// The nodes of `tree` and the `surface` node over them (its name, which no other node has).
fn closure_nodes(l: &mut Lowerer, tree: Tree) -> Result<(Vec<ShaderNode>, String), Error> {
    let mut name = "surface".to_string();
    while tree.nodes.iter().any(|n| n.name == name) {
        name.push('_');
    }
    let mut surface = ShaderNode {
        name: name.clone(),
        category: "surface".to_string(),
        port_type: PortType::SurfaceShader,
        inputs: Vec::new(),
        links: Vec::new(),
    };
    if let Some(transparency) = tree.transparency {
        let opacity = one_minus(l, transparency, "surface.opacity")?;
        surface.inputs.push(("opacity".to_string(), Arg::Node(opacity)));
    }
    if let Some(bsdf) = tree.bsdf {
        surface.links.push(("bsdf".to_string(), bsdf));
    }
    if let Some(edf) = tree.edf {
        surface.links.push(("edf".to_string(), edf));
    }
    let mut nodes = tree.nodes;
    nodes.push(surface);
    Ok((nodes, name))
}
