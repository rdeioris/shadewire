//! Finishing a dag for a front end without a document: the surface as a known model over fields, or as a closure
//! tree of shader-level nodes, through the same code a document's surface goes through.

use std::collections::HashMap;

use shadewire::{Dag, Error, Location};

use super::{CLOSURE_MODEL, SurfaceSource};
use crate::document::{Node, PortType};
use crate::elaborate::{Arg, Elaborator, Scope};
use crate::synthetic::input;

/// A shader-level node of a closure tree: a pbrlib lobe (`oren_nayar_diffuse_bsdf`, type `BSDF`), an operator
/// (`layer`, `mix`, `add`, `multiply`), or the `surface` node. `inputs` are its value inputs; `links` connect its
/// closure inputs to other nodes of the same tree by name.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderNode {
    pub name: String,
    pub category: String,
    pub port_type: PortType,
    pub inputs: Vec<(String, Arg)>,
    pub links: Vec<(String, String)>,
}

impl<'a> Elaborator<'a> {
    /// The dag finished as `model` over `fields` (missing fields take the nodedef's defaults): the model itself
    /// when the options accept it; its closure tree when they accept `surface` instead; else `NoTranslation`.
    pub fn finish_model(mut self, model: &str, fields: &[(&str, Arg)], origin: &str) -> Result<Dag, Error> {
        let origin = self.unique_origin(origin);
        let origin = origin.as_str();
        let at = Location { file: None, line: 0, path: origin.to_string() };
        let Some(synthetic) = self.synthetic else {
            return Err(Error::BadValue { at, message: "finish_model needs Elaborator::standalone".to_string() });
        };
        let (inputs, interface) = self.bind_args(fields, origin);
        let node = synthetic.node(Node {
            category: model.to_string(),
            name: origin.to_string(),
            port_type: PortType::SurfaceShader,
            nodedef: None,
            version: None,
            inputs,
            line: 0,
        });
        let scope = self.synthetic_scope(&[], interface);
        self.finish_at(&scope, node)
    }

    /// The dag finished as the closure tree under the node named `surface` among `nodes`.
    pub fn finish_closure(mut self, nodes: Vec<ShaderNode>, surface: &str) -> Result<Dag, Error> {
        let at = Location { file: None, line: 0, path: surface.to_string() };
        let Some(synthetic) = self.synthetic else {
            return Err(Error::BadValue { at, message: "finish_closure needs Elaborator::standalone".to_string() });
        };
        let mut interface = HashMap::new();
        let mut doc_nodes = Vec::new();
        for shader in &nodes {
            let args: Vec<(&str, Arg)> = shader.inputs.iter().map(|(n, a)| (n.as_str(), a.clone())).collect();
            let (mut inputs, bound) = self.bind_args(&args, &shader.name);
            interface.extend(bound);
            for (input_name, target) in &shader.links {
                let target_type =
                    nodes.iter().find(|n| &n.name == target).map(|n| n.port_type.clone()).ok_or_else(|| {
                        Error::BadValue { at: at.clone(), message: format!("{}: no node named {target}", shader.name) }
                    })?;
                let mut link = input(input_name, target_type);
                link.nodename = Some(target.clone());
                inputs.push(link);
            }
            doc_nodes.push(Node {
                category: shader.category.clone(),
                name: shader.name.clone(),
                port_type: shader.port_type.clone(),
                nodedef: None,
                version: None,
                inputs,
                line: 0,
            });
        }
        let group = synthetic.group(doc_nodes);
        let scope = self.synthetic_scope(group, interface);
        let node = group
            .iter()
            .find(|n| n.name == surface)
            .ok_or_else(|| Error::BadValue { at, message: format!("no node named {surface}") })?;
        self.finish_at(&scope, node)
    }

    /// What `elaborate_here` does once a document's surface is found, for a surface node of a synthetic scope.
    fn finish_at(mut self, scope: &Scope<'a>, node: &'a Node) -> Result<Dag, Error> {
        // A `surface` node is only a surface to a host that accepts closure trees; `surface_at` would otherwise
        // report it as "not a known shading model".
        if node.category == CLOSURE_MODEL && !self.accepts_closures {
            return Err(Error::NoTranslation { from: CLOSURE_MODEL.to_string(), to: self.accepted_models.clone() });
        }
        let source = self.surface_at(scope, node, 0)?;
        self.finish_source(source)
    }

    /// The dag finished with `source` as its surface: the fields of its model, or its closure tree for the
    /// `surface` model; `NoTranslation` when the options do not accept its model.
    pub(crate) fn finish_source(mut self, source: SurfaceSource<'a>) -> Result<Dag, Error> {
        let model = source.model().to_string();
        if !self.accepted_models.contains(&model) {
            return Err(Error::NoTranslation { from: model, to: self.accepted_models.clone() });
        }
        if model == CLOSURE_MODEL {
            let (fields, closure) = self.closure_surface(&source)?;
            self.dag.surface.fields = fields;
            self.dag.surface.closure = Some(closure);
        } else {
            self.dag.surface.fields = self.surface_fields(&source)?;
        }
        self.check_node_limit()?;
        self.dag.surface.model = model;
        self.dag.geomprop_defaults = self.geomprop_defaults.drain().collect();
        Ok(self.dag)
    }
}
