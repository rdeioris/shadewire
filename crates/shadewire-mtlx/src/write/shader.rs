//! The shader level of a document, before it is written: the model node, or the closure tree's nodes and the
//! `surface` node, each input with what it reads. A value input a reader would default to the same node is left
//! out.

use shadewire::{ClosureGraph, ClosureNode, ClosureType, Dag, Error, Location, NodeId, NodeKind, Type, Value};
use shadewire_nodes::Index;
use shadewire_nodes::document::{Node, NodeDef, PortDef, PortType};
use shadewire_nodes::elaborate::defaults::geomprop_read;
use shadewire_nodes::resolve_nodedef;

/// What one input of a shader-level node reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Read {
    /// A dag node, reached through the nodegraph or, when `value_allowed` (the port has no `defaultgeomprop`),
    /// written as a value.
    Field { node: NodeId, value_allowed: bool },
    /// A string setting.
    Text(String),
    /// Another shader-level node, by position.
    Closure(usize),
}

/// A shader-level node: its category, its output type and its inputs as `(name, type, read)`.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderNode {
    pub category: String,
    pub ty: String,
    pub inputs: Vec<(String, String, Read)>,
}

/// The shader-level nodes of `dag`: the closure tree's nodes in order then the `surface` node, or the model's
/// node alone. The shader is always last. The empty closure is no node: an input reading it is left unconnected,
/// which is what MaterialX makes of an unconnected closure input.
pub fn shader_nodes(dag: &Dag, index: &Index) -> Result<Vec<ShaderNode>, Error> {
    let Some(closure) = &dag.surface.closure else {
        let nd = nodedef(index, &dag.surface.model, PortType::SurfaceShader)?;
        let pairs: Vec<(&str, &str)> = dag.surface.fields.iter().map(|f| (f.name.as_str(), f.name.as_str())).collect();
        let inputs = value_inputs(dag, index, nd, &pairs);
        return Ok(vec![ShaderNode { category: dag.surface.model.clone(), ty: "surfaceshader".into(), inputs }]);
    };
    let mut positions = Vec::with_capacity(closure.nodes.len());
    let mut nodes = Vec::new();
    for id in 0..closure.nodes.len() {
        if matches!(closure.nodes[id], ClosureNode::Empty { .. }) {
            positions.push(None);
            continue;
        }
        positions.push(Some(nodes.len()));
        nodes.push(closure_node(dag, index, closure, &positions, id)?);
    }
    let surface = nodedef(index, "surface", PortType::SurfaceShader)?;
    let mut inputs = Vec::new();
    for (input, root) in [("bsdf", closure.bsdf), ("edf", closure.edf)] {
        inputs.extend(root.and_then(|root| closure_input(closure, &positions, input, root)));
    }
    // The surface node's own values are the fields named after its value inputs (`thin_walled`), and the opacity.
    let own: Vec<(&str, &str)> = surface
        .inputs
        .iter()
        .filter_map(|p| match (&p.port_type, p.name.as_str()) {
            (PortType::Value(_), "opacity") => closure.opacity.as_deref().map(|field| ("opacity", field)),
            (PortType::Value(_), name) => Some((name, name)),
            _ => None,
        })
        .collect();
    inputs.extend(value_inputs(dag, index, surface, &own));
    nodes.push(ShaderNode { category: "surface".into(), ty: "surfaceshader".into(), inputs });
    Ok(nodes)
}

/// Every dag node the shader level reads, once per read: what `Patterns::reach` is asked for.
pub fn shader_reads(dag: &Dag, index: &Index) -> Vec<NodeId> {
    let nodes = shader_nodes(dag, index).unwrap_or_default();
    let reads = nodes.into_iter().flat_map(|n| n.inputs);
    reads.filter_map(|(_, _, read)| if let Read::Field { node, .. } = read { Some(node) } else { None }).collect()
}

/// Input `name` reading closure node `id` (at shader-level node `positions[id]`); none for the empty closure.
fn closure_input(
    closure: &ClosureGraph,
    positions: &[Option<usize>],
    name: &str,
    id: u32,
) -> Option<(String, String, Read)> {
    let position = positions[id as usize]?;
    Some((name.to_string(), closure_name(closure.ty(id)).to_string(), Read::Closure(position)))
}

/// One closure node: a lobe with its value inputs, settings and closure inputs; an operator with its closures and
/// its weight, always written (its type picks the operator's nodedef).
fn closure_node(
    dag: &Dag,
    index: &Index,
    closure: &ClosureGraph,
    positions: &[Option<usize>],
    id: usize,
) -> Result<ShaderNode, Error> {
    let input = |name: &str, id: u32| closure_input(closure, positions, name, id);
    let ty = closure_name(closure.ty(id as u32));
    let (category, inputs) = match &closure.nodes[id] {
        ClosureNode::Lobe { kind, ty, inputs, settings, closures } => {
            let nd = nodedef(index, kind, PortType::Closure(closure_name(*ty).to_string()))?;
            let mut all: Vec<_> = closures.iter().filter_map(|(name, id)| input(name, *id)).collect();
            let by_field: Vec<(&str, &str)> = inputs.iter().map(|(i, f)| (i.as_str(), f.as_str())).collect();
            all.extend(value_inputs(dag, index, nd, &by_field));
            all.extend(settings.iter().map(|(name, text)| (name.clone(), "string".into(), Read::Text(text.clone()))));
            (kind.as_str(), all)
        }
        ClosureNode::Layer { top, base } => {
            ("layer", [input("top", *top), input("base", *base)].into_iter().flatten().collect())
        }
        ClosureNode::Mix { fg, bg, mix } => {
            let mut all: Vec<_> = [input("fg", *fg), input("bg", *bg)].into_iter().flatten().collect();
            all.extend(field_input(dag, None, "mix", mix));
            ("mix", all)
        }
        ClosureNode::Add { in1, in2 } => {
            ("add", [input("in1", *in1), input("in2", *in2)].into_iter().flatten().collect())
        }
        ClosureNode::Multiply { in1, in2 } => {
            let mut all: Vec<_> = input("in1", *in1).into_iter().collect();
            all.extend(field_input(dag, None, "in2", in2));
            ("multiply", all)
        }
        ClosureNode::Empty { .. } => unreachable!("the empty closure is no shader-level node"),
    };
    Ok(ShaderNode { category: category.to_string(), ty: ty.to_string(), inputs })
}

fn closure_name(ty: ClosureType) -> &'static str {
    match ty {
        ClosureType::Bsdf => "BSDF",
        ClosureType::Edf => "EDF",
        ClosureType::Vdf => "VDF",
    }
}

/// The inputs of a node of nodedef `nd` reading the fields `(input, field)`; an input whose field is what the
/// reader would default it to is left out.
fn value_inputs(dag: &Dag, index: &Index, nd: &NodeDef, pairs: &[(&str, &str)]) -> Vec<(String, String, Read)> {
    let mut out = Vec::new();
    for (input, field) in pairs {
        let port = nd.inputs.iter().find(|p| p.name == *input);
        let Some(found) = dag.surface.fields.iter().find(|f| f.name == *field) else { continue };
        if port.is_some_and(|port| is_default(dag, index, port, found.node)) {
            continue;
        }
        out.extend(field_input(dag, port, input, field));
    }
    out
}

/// Input `input` reading field `field`, typed by `port` when known, else by the field.
fn field_input(dag: &Dag, port: Option<&PortDef>, input: &str, field: &str) -> Option<(String, String, Read)> {
    let found = dag.surface.fields.iter().find(|f| f.name == field)?;
    let ty = match port.map(|p| &p.port_type) {
        Some(PortType::Value(ty)) => ty.mtlx_name(),
        _ => found.ty.mtlx_name(),
    };
    let value_allowed = port.is_none_or(|p| p.default_geomprop.is_none());
    Some((input.to_string(), ty.to_string(), Read::Field { node: found.node, value_allowed }))
}

/// Whether `node` is what an input of `port` left out reads: its `defaultgeomprop`, else its value (zero when
/// none).
fn is_default(dag: &Dag, index: &Index, port: &PortDef, node: NodeId) -> bool {
    let PortType::Value(ty) = port.port_type else { return false };
    match (&dag.node(node).kind, &port.default_geomprop) {
        (NodeKind::Context(read), Some(name)) => geomprop_read(index, name, ty).is_ok_and(|r| r == *read),
        (NodeKind::Const(value), None) => port_value(port, ty).is_some_and(|default| default == *value),
        _ => false,
    }
}

/// The value a port declares, as the reader parses it (zero when it declares none).
fn port_value(port: &PortDef, ty: Type) -> Option<Value> {
    match port.value.as_deref() {
        Some(text) if !text.trim().is_empty() => Value::parse(ty, text).ok(),
        _ => Some(Value::zero(ty)),
    }
}

/// The nodedef a node of `category` outputting `output` resolves to with no inputs given.
fn nodedef<'a>(index: &Index<'a>, category: &str, output: PortType) -> Result<&'a NodeDef, Error> {
    let node = Node {
        category: category.to_string(),
        name: category.to_string(),
        port_type: output,
        nodedef: None,
        version: None,
        inputs: Vec::new(),
        line: 0,
        source: None,
    };
    resolve_nodedef(index, &node, &[], &Location { file: None, line: 0, path: category.to_string(), source: None })
}
