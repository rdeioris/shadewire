//! Following a material's surfaceshader input to the shading model node:
//! through `dot` nodes, user nodegraph outputs, nodegraph-implemented shader
//! nodedefs, and same-model `mix` nodes. A host that accepts the `surface` model
//! also gets the pbrlib `surface` node, the closure tree under it, and the named
//! models it does not accept opened through their implementation graphs.

use std::rc::Rc;

use shadewire::{Error, Location};

use super::models::{CLOSURE_MODEL, KNOWN_MODELS};
use crate::document::{Document, Input, Node, NodeDef, PortType};
use crate::elaborate::{Elaborator, Scope};
use crate::resolve_nodedef::resolve_nodedef;

/// Deeper than any real document chains surface nodes; past it is a cycle.
const MAX_DEPTH: u32 = 64;

/// The mix node of a surface mix, for reading its `mix` weight.
#[derive(Clone)]
pub struct MixInput<'a> {
    pub node: &'a Node,
    pub nodedef: &'a NodeDef,
    pub scope: Scope<'a>,
}

/// Where a material's surface comes from. A sub-mix both sides of a mix
/// reach is one shared source.
#[derive(Clone)]
pub enum SurfaceSource<'a> {
    Model {
        node: &'a Node,
        nodedef: &'a NodeDef,
        scope: Scope<'a>,
    },
    Mix {
        fg: Rc<SurfaceSource<'a>>,
        bg: Rc<SurfaceSource<'a>>,
        mix_input: MixInput<'a>,
    },
    /// A pbrlib `surface` node: a closure tree, compiled as the `surface` model.
    Closure {
        node: &'a Node,
        nodedef: &'a NodeDef,
        scope: Scope<'a>,
    },
}

impl SurfaceSource<'_> {
    /// The shading model: the model node's category, or the mixed sides' model.
    pub fn model(&self) -> &str {
        match self {
            SurfaceSource::Model { node, .. } => &node.category,
            SurfaceSource::Mix { fg, .. } => fg.model(),
            SurfaceSource::Closure { .. } => CLOSURE_MODEL,
        }
    }

    /// `(scope id, node name)` of the model or mix node this source starts at.
    pub fn key(&self) -> (u32, String) {
        match self {
            SurfaceSource::Model { node, scope, .. } | SurfaceSource::Closure { node, scope, .. } => {
                (scope.id, node.name.clone())
            }
            SurfaceSource::Mix { mix_input, .. } => (mix_input.scope.id, mix_input.node.name.clone()),
        }
    }
}

/// The material to compile: the one named, or the document's only one.
pub fn pick_material<'a>(doc: &'a Document, name: Option<&str>) -> Result<&'a Node, Error> {
    let materials: Vec<&Node> = doc.nodes.iter().filter(|n| n.category == "surfacematerial").collect();
    match (name, materials.as_slice()) {
        (Some(name), _) => materials
            .iter()
            .copied()
            .find(|m| m.name == name)
            .ok_or_else(|| Error::UnknownMaterial { name: name.to_string() }),
        (None, []) => Err(Error::NoMaterial),
        (None, [only]) => Ok(only),
        (None, several) => Err(Error::AmbiguousMaterial { names: several.iter().map(|m| m.name.clone()).collect() }),
    }
}

impl<'a> Elaborator<'a> {
    /// The surface a material's `surfaceshader` input leads to.
    pub fn find_surface(&mut self, scope: &Scope<'a>, material: &'a Node) -> Result<SurfaceSource<'a>, Error> {
        self.follow_shader_input(scope, material, "surfaceshader", 0)
    }

    fn unsupported(&self, scope: &Scope<'a>, node: &Node, reason: &str) -> Error {
        Error::UnsupportedSurface {
            at: self.at(scope, node.line, scope.origin(&node.name)),
            node: node.name.clone(),
            reason: reason.to_string(),
        }
    }

    /// The surface behind shader input `input_name` of `node`.
    fn follow_shader_input(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        input_name: &str,
        depth: u32,
    ) -> Result<SurfaceSource<'a>, Error> {
        let at = self.at(scope, node.line, scope.origin(&node.name));
        if depth > MAX_DEPTH {
            return Err(Error::Cycle { at, node: node.name.clone() });
        }
        let Some(input) = node.inputs.iter().find(|i| i.name == input_name) else {
            return Err(self.unsupported(scope, node, &format!("no {input_name} connected")));
        };
        self.follow_connection(scope, node, input, &at, depth)
    }

    fn follow_connection(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        input: &'a Input,
        at: &Location,
        depth: u32,
    ) -> Result<SurfaceSource<'a>, Error> {
        if let Some(name) = &input.nodename {
            let target = scope
                .find_node(name)
                .ok_or_else(|| Error::BadValue { at: at.clone(), message: format!("no node named {name}") })?;
            return self.surface_at(scope, target, depth + 1);
        }
        if let Some(graph) = &input.nodegraph {
            let inner = self.user_graph_scope(graph, at)?;
            return self.follow_graph_output(&inner, input.output.as_deref(), at, depth + 1);
        }
        Err(self.unsupported(scope, node, &format!("{} is not connected to a shader node", input.name)))
    }

    /// The surface behind output `output` (or the only surfaceshader output) of the scope's graph.
    fn follow_graph_output(
        &mut self,
        scope: &Scope<'a>,
        output: Option<&str>,
        at: &Location,
        depth: u32,
    ) -> Result<SurfaceSource<'a>, Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Cycle { at: at.clone(), node: scope.path.clone() });
        }
        let graph =
            scope.graph.ok_or_else(|| Error::BadValue { at: at.clone(), message: "no graph in scope".to_string() })?;
        let found = match output {
            Some(name) => graph.outputs.iter().find(|o| o.name == name),
            None => graph.outputs.iter().find(|o| o.port_type == PortType::SurfaceShader),
        };
        let found = found.ok_or_else(|| Error::BadValue {
            at: at.clone(),
            message: format!("{} has no surfaceshader output {}", graph.name, output.unwrap_or("")),
        })?;
        let out_at = self.at(scope, found.line, scope.origin(&found.name));
        if let Some(name) = &found.nodename {
            let target = scope
                .find_node(name)
                .ok_or_else(|| Error::BadValue { at: out_at.clone(), message: format!("no node named {name}") })?;
            return self.surface_at(scope, target, depth + 1);
        }
        if let Some(name) = &found.nodegraph {
            let inner = self.user_graph_scope(name, &out_at)?;
            return self.follow_graph_output(&inner, found.output.as_deref(), &out_at, depth + 1);
        }
        Err(Error::UnsupportedSurface {
            at: out_at,
            node: found.name.clone(),
            reason: "output is not connected to a shader node".to_string(),
        })
    }

    /// What shader node `node` is, found once per scope and node: both sides
    /// of a mix may reach the same sub-mix.
    pub(crate) fn surface_at(
        &mut self,
        scope: &Scope<'a>,
        node: &'a Node,
        depth: u32,
    ) -> Result<SurfaceSource<'a>, Error> {
        let key = (scope.id, node.name.clone());
        if let Some(found) = self.surface_sources.get(&key) {
            return Ok(found.clone());
        }
        let found = self.find_surface_at(scope, node, depth)?;
        self.surface_sources.insert(key, found.clone());
        Ok(found)
    }

    /// What shader node `node` is: a model, a mix, a `dot`, or a graph-implemented nodedef.
    fn find_surface_at(&mut self, scope: &Scope<'a>, node: &'a Node, depth: u32) -> Result<SurfaceSource<'a>, Error> {
        let at = self.at(scope, node.line, scope.origin(&node.name));
        let is_shader = node.port_type == PortType::SurfaceShader;
        if KNOWN_MODELS.contains(&node.category.as_str()) {
            let nodedef = self.shader_nodedef(node, &at)?;
            let opens = self.accepts_closures
                && !self.accepted_models.contains(&node.category)
                && self.index.graph_for_nodedef(&nodedef.name).is_some();
            if !opens {
                return Ok(SurfaceSource::Model { node, nodedef, scope: scope.clone() });
            }
        }
        if node.category == CLOSURE_MODEL && is_shader && self.accepts_closures {
            let nodedef = self.shader_nodedef(node, &at)?;
            return Ok(SurfaceSource::Closure { node, nodedef, scope: scope.clone() });
        }
        if node.category == "mix" && is_shader {
            let fg = self.follow_shader_input(scope, node, "fg", depth + 1)?;
            let bg = self.follow_shader_input(scope, node, "bg", depth + 1)?;
            if fg.model() != bg.model() {
                return Err(self.unsupported(scope, node, "mix of different models"));
            }
            let nodedef = self.shader_nodedef(node, &at)?;
            let mix_input = MixInput { node, nodedef, scope: scope.clone() };
            return Ok(SurfaceSource::Mix { fg: Rc::new(fg), bg: Rc::new(bg), mix_input });
        }
        if node.category == "dot" && is_shader {
            return self.follow_shader_input(scope, node, "in", depth + 1);
        }
        if !self.index.nodedefs_for_category(&node.category).is_empty() {
            let nodedef = self.shader_nodedef(node, &at)?;
            if let Some(graph) = self.index.graph_for_nodedef(&nodedef.name) {
                let interface = self.bind_interface(scope, node, nodedef, scope.user)?;
                let path = scope.origin(&node.name);
                let file = self.index.graph_file(graph);
                let child = Scope {
                    id: self.new_scope_id(),
                    graph: Some(graph),
                    nodes: &graph.nodes,
                    interface,
                    user: false,
                    path,
                    file,
                };
                return self.follow_graph_output(&child, None, &at, depth + 1);
            }
        }
        Err(self.unsupported(scope, node, "not a known shading model"))
    }

    fn shader_nodedef(&self, node: &Node, at: &Location) -> Result<&'a NodeDef, Error> {
        let input_types: Vec<(String, PortType)> =
            node.inputs.iter().map(|i| (i.name.clone(), i.port_type.clone())).collect();
        resolve_nodedef(&self.index, node, &input_types, at)
    }
}
