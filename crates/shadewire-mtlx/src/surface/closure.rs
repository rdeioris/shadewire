//! The closure tree under a pbrlib `surface` node: BSDF, EDF and VDF lobes combined by `layer`, `mix`,
//! `add` and `multiply`. Every value the tree reads becomes a surface field; the tree itself is the
//! `ClosureGraph` the host walks.

use std::collections::{BTreeMap, HashMap};

use shadewire::{
    ClosureGraph, ClosureId, ClosureNode, ClosureType, Error, Location, Node as IrNode, NodeId, NodeKind, Op,
    SurfaceField, Type, Value,
};

use super::find::SurfaceSource;
use super::models::fields;
use crate::document::{Input, Node, NodeDef, PortType};
use crate::elaborate::{Elaborator, Scope};
use crate::resolve_nodedef::resolve_nodedef;

/// Deeper than any real closure tree; past it is a cycle.
const MAX_DEPTH: u32 = 256;

/// What one side of a surface gives: its closure roots and its own values (`opacity`, `thin_walled`).
struct SurfaceClosures {
    bsdf: Option<ClosureId>,
    edf: Option<ClosureId>,
    values: Vec<(String, Type, NodeId)>,
}

/// The tree and fields being built for one material.
struct Builder<'a> {
    graph: ClosureGraph,
    fields: Vec<SurfaceField>,
    /// Closure node outputs already built, by `(scope id, node name, output name)`.
    memo: HashMap<(u32, String, String), Option<ClosureId>>,
    /// Instances of graph-implemented closure nodedefs, by the id of their inlined scope: an `interfacename`
    /// on a closure input there reads the instance node's input in its own scope.
    instances: HashMap<u32, (Scope<'a>, &'a Node)>,
}

impl Builder<'_> {
    /// A field for input `input` of the closure node about to be added; returns its name.
    fn field(&mut self, input: &str, ty: Type, node: NodeId) -> String {
        let name = format!("c{}_{input}", self.graph.nodes.len());
        self.fields.push(SurfaceField { name: name.clone(), ty, node });
        name
    }
}

fn closure_type(port_type: &PortType) -> Option<ClosureType> {
    match port_type {
        PortType::Closure(name) => match name.as_str() {
            "BSDF" => Some(ClosureType::Bsdf),
            "EDF" => Some(ClosureType::Edf),
            "VDF" => Some(ClosureType::Vdf),
            _ => None,
        },
        _ => None,
    }
}

impl<'a> Elaborator<'a> {
    /// The fields and closure tree of a `surface`-model source: one `surface` node, or a mix of them.
    pub fn closure_surface(&mut self, source: &SurfaceSource<'a>) -> Result<(Vec<SurfaceField>, ClosureGraph), Error> {
        let mut b = Builder {
            graph: ClosureGraph::default(),
            fields: Vec::new(),
            memo: HashMap::new(),
            instances: HashMap::new(),
        };
        let top = self.surface_closures(&mut b, source)?;
        b.graph.bsdf = top.bsdf;
        b.graph.edf = top.edf;
        for (name, ty, node) in top.values {
            if name == "opacity" {
                b.graph.opacity = Some(name.clone());
            }
            b.fields.push(SurfaceField { name, ty, node });
        }
        Ok((b.fields, b.graph))
    }

    fn surface_closures(&mut self, b: &mut Builder<'a>, source: &SurfaceSource<'a>) -> Result<SurfaceClosures, Error> {
        match source {
            SurfaceSource::Closure { node, nodedef, scope } => {
                let mut values = Vec::new();
                for (name, ty) in fields(nodedef) {
                    let id = self.surface_input_value(scope, node, nodedef, &name, scope.user)?;
                    values.push((name, ty, id));
                }
                let bsdf = self.closure_input(b, scope, node, "bsdf", 0)?;
                let edf = self.closure_input(b, scope, node, "edf", 0)?;
                Ok(SurfaceClosures { bsdf, edf, values })
            }
            SurfaceSource::Mix { fg, bg, mix_input } => {
                let fg = self.surface_closures(b, fg)?;
                let bg = self.surface_closures(b, bg)?;
                let scope = &mix_input.scope;
                let weight = self.surface_input_value(scope, mix_input.node, mix_input.nodedef, "mix", scope.user)?;
                let origin = scope.origin(&mix_input.node.name);
                let bsdf = self.mix_closures(b, fg.bsdf, bg.bsdf, weight, &origin);
                let edf = self.mix_closures(b, fg.edf, bg.edf, weight, &origin);
                let mut values = Vec::new();
                for ((name, ty, f), (_, _, g)) in fg.values.into_iter().zip(bg.values) {
                    let node = self.mix_field(f, g, weight, ty, format!("{origin}.{name}"));
                    values.push((name, ty, node));
                }
                Ok(SurfaceClosures { bsdf, edf, values })
            }
            SurfaceSource::Model { node, scope, .. } => Err(Error::UnsupportedSurface {
                at: self.at(scope, node.line, scope.origin(&node.name)),
                node: node.name.clone(),
                reason: "a shading model mixed with a surface node".to_string(),
            }),
        }
    }

    /// `mix(bg, fg, weight)` of two optional closures; a missing side scatters nothing.
    fn mix_closures(
        &mut self,
        b: &mut Builder<'a>,
        fg: Option<ClosureId>,
        bg: Option<ClosureId>,
        weight: NodeId,
        origin: &str,
    ) -> Option<ClosureId> {
        match (fg, bg) {
            (Some(fg), Some(bg)) => {
                let mix = b.field("mix", Type::Float, weight);
                Some(b.graph.add(ClosureNode::Mix { fg, bg, mix }))
            }
            (Some(fg), None) => {
                let in2 = b.field("in2", Type::Float, weight);
                Some(b.graph.add(ClosureNode::Multiply { in1: fg, in2 }))
            }
            (None, Some(bg)) => {
                let one = self.dag.add(IrNode {
                    kind: NodeKind::Const(Value::Float(1.0)),
                    ty: Type::Float,
                    origin: origin.to_string(),
                });
                let kind = NodeKind::Op { op: Op::Subtract, args: vec![one, weight] };
                let rest = self.dag.add(IrNode { kind, ty: Type::Float, origin: origin.to_string() });
                let in2 = b.field("in2", Type::Float, rest);
                Some(b.graph.add(ClosureNode::Multiply { in1: bg, in2 }))
            }
            (None, None) => None,
        }
    }

    fn closure_error(&self, scope: &Scope<'a>, node: &Node, reason: String) -> Error {
        Error::UnsupportedSurface {
            at: self.at(scope, node.line, scope.origin(&node.name)),
            node: node.name.clone(),
            reason,
        }
    }

    /// The closure behind closure input `input_name` of `node`; `None` when nothing is connected.
    fn closure_input(
        &mut self,
        b: &mut Builder<'a>,
        scope: &Scope<'a>,
        node: &'a Node,
        input_name: &str,
        depth: u32,
    ) -> Result<Option<ClosureId>, Error> {
        match node.inputs.iter().find(|i| i.name == input_name) {
            Some(input) => self.closure_connection(b, scope, input, depth),
            None => Ok(None),
        }
    }

    fn closure_connection(
        &mut self,
        b: &mut Builder<'a>,
        scope: &Scope<'a>,
        input: &'a Input,
        depth: u32,
    ) -> Result<Option<ClosureId>, Error> {
        let at = self.at(scope, input.line, format!("{}.{}", scope.path, input.name));
        if depth > MAX_DEPTH {
            return Err(Error::Cycle { at, node: input.name.clone() });
        }
        if let Some(name) = &input.nodename {
            let target = scope
                .find_node(name)
                .ok_or_else(|| Error::BadValue { at: at.clone(), message: format!("no node named {name}") })?;
            return self.closure_at(b, scope, target, input.output.as_deref(), depth + 1);
        }
        if let Some(graph) = &input.nodegraph {
            let inner = self.user_graph_scope(graph, &at)?;
            return self.closure_graph_output(b, &inner, input.output.as_deref(), &at, depth + 1);
        }
        if let Some(name) = &input.interfacename {
            return match b.instances.get(&scope.id).cloned() {
                Some((outer, instance)) => self.closure_input(b, &outer, instance, name, depth + 1),
                None => Ok(None),
            };
        }
        Ok(None)
    }

    /// The closure behind output `output` (or the first closure output) of the scope's graph.
    fn closure_graph_output(
        &mut self,
        b: &mut Builder<'a>,
        scope: &Scope<'a>,
        output: Option<&str>,
        at: &Location,
        depth: u32,
    ) -> Result<Option<ClosureId>, Error> {
        let graph =
            scope.graph.ok_or_else(|| Error::BadValue { at: at.clone(), message: "no graph in scope".to_string() })?;
        let found = match output {
            Some(name) => graph.outputs.iter().find(|o| o.name == name),
            None => graph.outputs.iter().find(|o| closure_type(&o.port_type).is_some()),
        };
        let found = found.ok_or_else(|| Error::BadValue {
            at: at.clone(),
            message: format!("{} has no closure output {}", graph.name, output.unwrap_or("")),
        })?;
        let out_at = self.at(scope, found.line, scope.origin(&found.name));
        if let Some(name) = &found.nodename {
            let target = scope
                .find_node(name)
                .ok_or_else(|| Error::BadValue { at: out_at.clone(), message: format!("no node named {name}") })?;
            return self.closure_at(b, scope, target, found.output.as_deref(), depth + 1);
        }
        if let Some(name) = &found.nodegraph {
            let inner = self.user_graph_scope(name, &out_at)?;
            return self.closure_graph_output(b, &inner, found.output.as_deref(), &out_at, depth + 1);
        }
        Ok(None)
    }

    /// The closure output `output` of `node` builds, built once per scope.
    fn closure_at(
        &mut self,
        b: &mut Builder<'a>,
        scope: &Scope<'a>,
        node: &'a Node,
        output: Option<&str>,
        depth: u32,
    ) -> Result<Option<ClosureId>, Error> {
        let key = (scope.id, node.name.clone(), output.unwrap_or("").to_string());
        if let Some(found) = b.memo.get(&key) {
            return Ok(*found);
        }
        let built = self.build_closure(b, scope, node, output, depth)?;
        b.memo.insert(key, built);
        Ok(built)
    }

    fn closure_nodedef(&self, scope: &Scope<'a>, node: &Node) -> Result<&'a NodeDef, Error> {
        let at = self.at(scope, node.line, scope.origin(&node.name));
        let input_types: Vec<(String, PortType)> =
            node.inputs.iter().map(|i| (i.name.clone(), i.port_type.clone())).collect();
        resolve_nodedef(&self.index, node, &input_types, &at)
    }

    fn build_closure(
        &mut self,
        b: &mut Builder<'a>,
        scope: &Scope<'a>,
        node: &'a Node,
        output: Option<&str>,
        depth: u32,
    ) -> Result<Option<ClosureId>, Error> {
        if closure_type(&node.port_type).is_none() {
            return Err(self.closure_error(scope, node, format!("{} is not a BSDF, EDF or VDF", node.category)));
        }
        let d = depth + 1;
        match node.category.as_str() {
            "dot" => self.closure_input(b, scope, node, "in", d),
            "layer" => {
                let top = self.closure_input(b, scope, node, "top", d)?;
                let base = self.closure_input(b, scope, node, "base", d)?;
                Ok(match (top, base) {
                    (Some(top), Some(base)) => Some(b.graph.add(ClosureNode::Layer { top, base })),
                    (one, None) | (None, one) => one,
                })
            }
            "add" => {
                let in1 = self.closure_input(b, scope, node, "in1", d)?;
                let in2 = self.closure_input(b, scope, node, "in2", d)?;
                Ok(match (in1, in2) {
                    (Some(in1), Some(in2)) => Some(b.graph.add(ClosureNode::Add { in1, in2 })),
                    (one, None) | (None, one) => one,
                })
            }
            "mix" => {
                let nd = self.closure_nodedef(scope, node)?;
                let fg = self.closure_input(b, scope, node, "fg", d)?;
                let bg = self.closure_input(b, scope, node, "bg", d)?;
                let weight = self.surface_input_value(scope, node, nd, "mix", scope.user)?;
                Ok(self.mix_closures(b, fg, bg, weight, &scope.origin(&node.name)))
            }
            "multiply" => {
                let nd = self.closure_nodedef(scope, node)?;
                let Some(in1) = self.closure_input(b, scope, node, "in1", d)? else { return Ok(None) };
                let port = nd.inputs.iter().find(|p| p.name == "in2");
                let Some(PortType::Value(ty)) = port.map(|p| p.port_type.clone()) else {
                    return Err(self.closure_error(scope, node, format!("{} has no value input in2", nd.name)));
                };
                let weight = self.surface_input_value(scope, node, nd, "in2", scope.user)?;
                let in2 = b.field("in2", ty, weight);
                Ok(Some(b.graph.add(ClosureNode::Multiply { in1, in2 })))
            }
            _ => self.closure_leaf_or_graph(b, scope, node, output, d),
        }
    }

    /// A lobe (a nodedef with a native implementation), or a graph-implemented closure nodedef inlined.
    fn closure_leaf_or_graph(
        &mut self,
        b: &mut Builder<'a>,
        scope: &Scope<'a>,
        node: &'a Node,
        output: Option<&str>,
        depth: u32,
    ) -> Result<Option<ClosureId>, Error> {
        let nd = self.closure_nodedef(scope, node)?;
        let at = self.at(scope, node.line, scope.origin(&node.name));
        if let Some(graph) = self.index.graph_for_nodedef(&nd.name) {
            let interface = self.bind_interface(scope, node, nd, scope.user)?;
            let child = Scope {
                id: self.new_scope_id(),
                graph: Some(graph),
                nodes: &graph.nodes,
                interface,
                user: false,
                path: scope.origin(&node.name),
                file: self.index.graph_file(graph),
            };
            b.instances.insert(child.id, (scope.clone(), node));
            if !self.inlining.insert(nd.name.clone()) {
                return Err(Error::Cycle { at, node: node.name.clone() });
            }
            let result = self.closure_graph_output(b, &child, output, &at, depth);
            self.inlining.remove(&nd.name);
            return result;
        }
        let ty = closure_type(&nd.outputs.first().map_or(PortType::Other(String::new()), |o| o.port_type.clone()))
            .ok_or_else(|| self.closure_error(scope, node, format!("{} does not output a closure", nd.name)))?;
        let mut inputs = BTreeMap::new();
        let mut settings = BTreeMap::new();
        // Closure inputs first: their nodes come before this one, so its id (and its field names) is known after.
        let mut closures = BTreeMap::new();
        for port in nd.inputs.iter().filter(|p| matches!(p.port_type, PortType::Closure(_))) {
            if let Some(child) = self.closure_input(b, scope, node, &port.name, depth)? {
                closures.insert(port.name.clone(), child);
            }
        }
        for port in &nd.inputs {
            match &port.port_type {
                PortType::Value(_) => {
                    let id = self.surface_input_value(scope, node, nd, &port.name, scope.user)?;
                    let field_ty = self.dag.node(id).ty;
                    inputs.insert(port.name.clone(), b.field(&port.name, field_ty, id));
                }
                PortType::String => {
                    if let Some(text) = self.text_input(scope, node, nd, &port.name)? {
                        settings.insert(port.name.clone(), text.text);
                    }
                }
                _ => {}
            }
        }
        Ok(Some(b.graph.add(ClosureNode::Lobe { kind: node.category.clone(), ty, inputs, settings, closures })))
    }
}
