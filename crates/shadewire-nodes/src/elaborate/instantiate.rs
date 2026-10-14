//! Library nodes for a front end without a document: a synthetic node whose inputs are interface bindings to
//! IR nodes already made, read by the same code that reads document nodes.

use std::collections::HashMap;

use shadewire::{ContextRead, Error, Location, Node as IrNode, NodeId, NodeKind, Type, Value};

use super::{Binding, Elaborator, Scope, TextValue};
use crate::document::{Input, Node, PortType, empty_document};
use crate::library::standard;
use crate::options::IrOptions;
use crate::surface::CLOSURE_MODEL;
use crate::synthetic::{Synthetic, input};

/// What a front end binds a node input to.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    /// An IR node already in the dag.
    Node(NodeId),
    /// A value, baked in as a constant (use `Elaborator::authored` first for a uniform).
    Value(Value),
    /// A string input (`uaddressmode`, `space`, `geomprop`).
    Text(String),
    /// A filename input and the colorspace the file is in (`None`: the working space).
    File { path: String, colorspace: Option<String> },
}

impl<'a> Elaborator<'a> {
    /// An elaborator over the standard library alone, for a front end that lowers its own graph: nodes it asks
    /// for live in `synthetic`. `options.models` are the accepted models, as for a document.
    ///
    /// Lowering recurses along the graph; a front end opening deep graphs (a model into its lobes) should call
    /// this on a thread with a large stack, as `elaborate` does with `STACK_BYTES`. `Synthetic` is not `Send`:
    /// make both on that thread.
    pub fn standalone(synthetic: &'a Synthetic, options: &'a IrOptions) -> Elaborator<'a> {
        let mut elaborator = Elaborator::new(empty_document(), standard(), options, "");
        elaborator.synthetic = Some(synthetic);
        elaborator.accepted_models = options.models.clone();
        elaborator.accepts_closures = options.models.iter().any(|m| m == CLOSURE_MODEL);
        elaborator
    }

    /// Output `out` of a `category` node over `inputs`; inputs not given take the nodedef's defaults. `origin`
    /// names the node in the IR. A reused `origin` gets a numeric suffix (`Mix`, `Mix_2`, ...) so each instance
    /// keeps its own compound.
    pub fn instantiate(
        &mut self,
        category: &str,
        out: Type,
        inputs: &[(&str, Arg)],
        origin: &str,
    ) -> Result<NodeId, Error> {
        self.instantiate_node(category, PortType::Value(out), inputs, None, origin)
    }

    /// Output `output` of a multioutput `category` node (`separate3`'s `outy`).
    pub fn instantiate_output(
        &mut self,
        category: &str,
        output: &str,
        inputs: &[(&str, Arg)],
        origin: &str,
    ) -> Result<NodeId, Error> {
        self.instantiate_node(category, PortType::MultiOutput, inputs, Some(output), origin)
    }

    fn instantiate_node(
        &mut self,
        category: &str,
        port_type: PortType,
        inputs: &[(&str, Arg)],
        output: Option<&str>,
        origin: &str,
    ) -> Result<NodeId, Error> {
        let origin = self.unique_origin(origin);
        let origin = origin.as_str();
        let at = Location { file: None, line: 0, path: origin.to_string() };
        let Some(synthetic) = self.synthetic else {
            return Err(Error::BadValue { at, message: "instantiate needs Elaborator::standalone".to_string() });
        };
        let (doc_inputs, interface) = self.bind_args(inputs, origin);
        let node = synthetic.node(Node {
            category: category.to_string(),
            name: origin.to_string(),
            port_type,
            nodedef: None,
            version: None,
            inputs: doc_inputs,
            line: 0,
        });
        let scope = self.synthetic_scope(&[], interface);
        let id = self.instance(&scope, node, output, &at)?;
        self.check_node_limit()?;
        Ok(id)
    }

    /// `origin`, then `origin_2`, `origin_3`, ... for later synthetic nodes of the same origin.
    pub(crate) fn unique_origin(&mut self, origin: &str) -> String {
        let mut unique = origin.to_string();
        let mut n = 2;
        while self.synthetic_origins.contains(&unique) {
            unique = format!("{origin}_{n}");
            n += 1;
        }
        self.synthetic_origins.insert(unique.clone());
        unique
    }

    /// Document inputs for `args`, each an `interfacename` into the returned bindings.
    pub(crate) fn bind_args(&mut self, args: &[(&str, Arg)], origin: &str) -> (Vec<Input>, HashMap<String, Binding>) {
        let mut doc_inputs = Vec::new();
        let mut interface = HashMap::new();
        for (name, arg) in args {
            let key = format!("{origin}.{name}");
            let (port_type, binding) = match arg {
                Arg::Node(id) => (PortType::Value(self.dag.node(*id).ty), Binding::Node(*id)),
                Arg::Value(value) => {
                    let id = self.constant(value.clone(), &key);
                    (PortType::Value(value.ty()), Binding::Node(id))
                }
                Arg::Text(text) => {
                    (PortType::String, Binding::Text(TextValue { text: text.clone(), colorspace: None }))
                }
                Arg::File { path, colorspace } => (
                    PortType::Filename,
                    Binding::Text(TextValue { text: path.clone(), colorspace: colorspace.clone() }),
                ),
            };
            let mut doc_input = input(name, port_type);
            doc_input.interfacename = Some(key.clone());
            doc_inputs.push(doc_input);
            interface.insert(key, binding);
        }
        (doc_inputs, interface)
    }

    /// A scope over synthetic `nodes` with `interface` bound; its path is empty, so node names are origins.
    pub(crate) fn synthetic_scope(&mut self, nodes: &'a [Node], interface: HashMap<String, Binding>) -> Scope<'a> {
        Scope { id: self.new_scope_id(), graph: None, nodes, interface, user: true, path: String::new(), file: None }
    }

    pub fn constant(&mut self, value: Value, origin: &str) -> NodeId {
        let ty = value.ty();
        self.dag.add(IrNode { kind: NodeKind::Const(value), ty, origin: origin.to_string() })
    }

    /// A value a front end's user authored: a uniform named after `name` (`name_2`, ... when taken) when the
    /// options keep values as uniforms, else a constant.
    pub fn authored(&mut self, name: &str, value: Value, origin: &str) -> NodeId {
        if !self.options.values_as_uniforms {
            return self.constant(value, origin);
        }
        let ty = value.ty();
        let name = self.unique_uniform_name(name);
        self.dag.add(IrNode { kind: NodeKind::Uniform { name, value }, ty, origin: origin.to_string() })
    }

    pub fn context(&mut self, read: ContextRead, origin: &str) -> NodeId {
        let ty = read.ty();
        self.dag.add(IrNode { kind: NodeKind::Context(read), ty, origin: origin.to_string() })
    }
}
