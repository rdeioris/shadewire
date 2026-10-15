//! Lowering: a flattened Blender graph becomes IR through `shadewire-nodes`, one node output at a time, from the
//! active Material Output's Surface back to the values it reads.

pub mod fallback;
pub mod lobes;
pub mod principled;
pub mod surface;
pub mod values;

#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod lobes_tests;
#[cfg(test)]
mod principled_tests;
#[cfg(test)]
pub(crate) mod tests;

use std::collections::{HashMap, HashSet};

use shadewire::{Error, NodeId, SourceInfo, Type, Value};
use shadewire_nodes::elaborate::STACK_BYTES;
use shadewire_nodes::{Arg, Elaborator, IrOptions, MaterialIr, Synthetic};

use crate::document::{Document, Image, Material, Socket, at};
use crate::graph::{FlatNode, Graph, VALUE_KIND};
use surface::Shading;
use values::{socket_type, socket_value, uniform_name};

/// The Rec. 709 luma coefficients Blender weighs a color by when a color socket feeds a float one.
const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

/// The state of one material's lowering: the elaborator making the IR, the graph it reads, and what it made so far.
pub struct Lowerer<'a> {
    pub e: Elaborator<'a>,
    pub graph: &'a Graph,
    pub material: &'a str,
    pub images: &'a [Image],
    /// `(node, output)` to the IR node it lowered to.
    memo: HashMap<(String, String), NodeId>,
    in_progress: HashSet<(String, String)>,
    /// Shader node name to the shading it carries, so a shader reached twice is lowered (and warned about) once.
    pub shaded: HashMap<String, Shading>,
    pub shading_in_progress: HashSet<String>,
    pub warnings: Vec<String>,
}

impl<'a> Lowerer<'a> {
    pub fn new(e: Elaborator<'a>, graph: &'a Graph, material: &'a str, images: &'a [Image]) -> Lowerer<'a> {
        Lowerer {
            e,
            graph,
            material,
            images,
            memo: HashMap::new(),
            in_progress: HashSet::new(),
            shaded: HashMap::new(),
            shading_in_progress: HashSet::new(),
            warnings: Vec::new(),
        }
    }

    /// Output `socket` of the graph node `node`, lowered once.
    pub fn output(&mut self, node: &str, socket: &str) -> Result<NodeId, Error> {
        let key = (node.to_string(), socket.to_string());
        if let Some(&id) = self.memo.get(&key) {
            return Ok(id);
        }
        let graph = self.graph;
        let Some(flat) = graph.node(node) else {
            return Err(Error::BadValue { at: at(node), message: format!("no node named {node}") });
        };
        if !self.in_progress.insert(key.clone()) {
            return Err(Error::Cycle { at: at(node), node: node.to_string() });
        }
        let lowered = self.dispatch(flat, socket);
        self.in_progress.remove(&key);
        let id = lowered?;
        self.memo.insert(key, id);
        Ok(id)
    }

    /// The node's handler, by Blender's `bl_idname`.
    fn dispatch(&mut self, node: &'a FlatNode, socket: &str) -> Result<NodeId, Error> {
        let own = |name: &str| node.output(name).ok_or_else(|| missing_output(node, name));
        match node.kind.as_str() {
            VALUE_KIND => {
                let out = node.outputs.first().ok_or_else(|| missing_output(node, socket))?;
                Ok(self.value(node, out, &node.name))
            }
            "ShaderNodeValue" => Ok(self.value(node, own("Value")?, &node.name)),
            "ShaderNodeRGB" => Ok(self.value(node, own("Color")?, &node.name)),
            _ => fallback::default_output(self, node, socket),
        }
    }

    /// Input `socket` of `node` as `ty`: what is linked to it, converted, or its authored value; a socket the
    /// document does not have is zero. A linked number read as an integer is floored, with a warning.
    pub fn input(&mut self, node: &FlatNode, socket: &str, ty: Type) -> Result<NodeId, Error> {
        let origin = format!("{}.{socket}", node.name);
        let graph = self.graph;
        if let Some((from, out)) = graph.source(&node.name, socket) {
            let id = self.output(from, out)?;
            let from_ty = self.e.dag.node(id).ty;
            if ty == Type::Integer && !matches!(from_ty, Type::Integer | Type::Boolean) {
                self.warn(node, format!("{socket} read as an integer by floor"));
            }
            return self.convert(id, ty, &origin);
        }
        match node.input(socket) {
            Some(s) if socket_value(s).is_some() => {
                let id = self.value(node, s, &origin);
                self.convert(id, ty, &origin)
            }
            _ => Ok(self.e.constant(Value::zero(ty), &origin)),
        }
    }

    /// Input `socket` of `node` as `ty` when something is linked to it.
    pub fn input_if_linked(&mut self, node: &FlatNode, socket: &str, ty: Type) -> Result<Option<NodeId>, Error> {
        if self.linked(node, socket) { self.input(node, socket, ty).map(Some) } else { Ok(None) }
    }

    pub fn linked(&self, node: &FlatNode, socket: &str) -> bool {
        self.graph.source(&node.name, socket).is_some()
    }

    /// `id` as `to`, by the conversions Blender makes when a link joins sockets of different types: a number
    /// becomes a boolean when greater than zero, and an integer by `floor`.
    pub fn convert(&mut self, id: NodeId, to: Type, origin: &str) -> Result<NodeId, Error> {
        let from = self.e.dag.node(id).ty;
        if from == to {
            return Ok(id);
        }
        match (from, to) {
            (Type::Color3, Type::Float) => {
                let coeffs = Arg::Value(Value::Color3(LUMA));
                let luma =
                    self.op("luminance", Type::Color3, &[("in", Arg::Node(id)), ("lumacoeffs", coeffs)], origin)?;
                let index = Arg::Value(Value::Integer(0));
                self.op("extract", Type::Float, &[("in", Arg::Node(luma)), ("index", index)], origin)
            }
            (Type::Vector3, Type::Float) => {
                let third = Arg::Value(Value::Vector3([1.0 / 3.0; 3]));
                self.op("dotproduct", Type::Float, &[("in1", Arg::Node(id)), ("in2", third)], origin)
            }
            (Type::Color4, Type::Float) => {
                let color = self.op("convert", Type::Color3, &[("in", Arg::Node(id))], origin)?;
                self.convert(color, Type::Float, origin)
            }
            (Type::Integer, Type::Boolean) | (Type::Boolean, Type::Integer) => {
                self.op("convert", to, &[("in", Arg::Node(id))], origin)
            }
            // Blender reads a number as true when it is greater than zero.
            (_, Type::Boolean) => {
                let x = self.convert(id, Type::Float, origin)?;
                let zero = Arg::Value(Value::Float(0.0));
                self.op("ifgreater", Type::Boolean, &[("value1", Arg::Node(x)), ("value2", zero)], origin)
            }
            (_, Type::Integer) => {
                let x = self.convert(id, Type::Float, origin)?;
                self.op("floor", Type::Integer, &[("in", Arg::Node(x))], origin)
            }
            (Type::Float | Type::Integer | Type::Boolean, _)
            | (Type::Color3, Type::Vector3)
            | (Type::Vector3, Type::Color3)
            | (Type::Color4, Type::Color3)
            | (Type::Vector2, Type::Vector3)
            | (Type::Vector3, Type::Vector2) => self.op("convert", to, &[("in", Arg::Node(id))], origin),
            _ => Err(Error::TypeMismatch {
                at: at(origin),
                expected: to.mtlx_name().to_string(),
                found: from.mtlx_name().to_string(),
            }),
        }
    }

    /// Output `out` of a `category` node over `inputs`, through the elaborator.
    pub fn op(&mut self, category: &str, out: Type, inputs: &[(&str, Arg)], origin: &str) -> Result<NodeId, Error> {
        self.e.instantiate(category, out, inputs, origin)
    }

    /// The value `socket` of `node` holds, as authored (a uniform when the options keep values as uniforms); zero
    /// when it holds none.
    pub fn value(&mut self, node: &FlatNode, socket: &Socket, origin: &str) -> NodeId {
        match socket_value(socket) {
            Some(value) => self.e.authored(&uniform_name(&node.name, &socket.name), value, origin),
            None => {
                let ty = socket_type(&socket.kind).unwrap_or(Type::Float);
                self.e.constant(Value::zero(ty), origin)
            }
        }
    }

    /// Records `{kind} "{name}": {what}`.
    pub fn warn(&mut self, node: &FlatNode, what: impl Into<String>) {
        self.warnings.push(format!("{} \"{}\": {}", node.kind, node.name, what.into()));
    }

    /// A constant the handler itself introduces.
    pub fn float(&mut self, x: f32, origin: &str) -> NodeId {
        self.e.constant(Value::Float(x), origin)
    }
}

fn missing_output(node: &FlatNode, socket: &str) -> Error {
    Error::BadValue { at: at(&node.name), message: format!("{} \"{}\" has no output {socket}", node.kind, node.name) }
}

/// `material` of `doc` lowered for `options`, on a thread with `STACK_BYTES` of stack (the caller's thread when
/// none can be made): flattening and lowering both recurse along the graph.
pub fn lower(doc: &Document, material: &Material, options: &IrOptions) -> Result<MaterialIr, Error> {
    std::thread::scope(|threads| {
        let builder = std::thread::Builder::new().name("shadewire-blender".to_string()).stack_size(STACK_BYTES);
        match builder.spawn_scoped(threads, || lower_here(doc, material, options)) {
            Ok(handle) => match handle.join() {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => lower_here(doc, material, options),
        }
    })
}

fn lower_here(doc: &Document, material: &Material, options: &IrOptions) -> Result<MaterialIr, Error> {
    let (graph, mut warnings) = Graph::flatten(doc, material)?;
    let synthetic = Synthetic::new();
    let e = Elaborator::standalone(&synthetic, options);
    let mut l = Lowerer::new(e, &graph, &material.name, &doc.images);
    let Some(output) = graph.output.as_deref().and_then(|name| graph.node(name)) else {
        return Err(Error::UnsupportedSurface {
            at: at(&material.name),
            node: material.name.clone(),
            reason: "no active Material Output".to_string(),
        });
    };
    for socket in ["Volume", "Displacement"] {
        if l.linked(output, socket) {
            l.warn(output, format!("{socket} ignored"));
        }
    }
    let shading = match graph.source(&output.name, "Surface") {
        Some((from, socket)) => {
            let Some(node) = graph.node(from) else {
                return Err(Error::BadValue { at: at(from), message: format!("no node named {from}") });
            };
            surface::shading(&mut l, node, socket)?
        }
        None => Shading::Nothing,
    };
    let (dag, lowered) = surface::finish(l, shading, output)?;
    warnings.extend(lowered);
    let source = SourceInfo { source: "blender".to_string(), mtlx_version: doc.producer.clone() };
    Ok(MaterialIr { dag, source, warnings })
}
