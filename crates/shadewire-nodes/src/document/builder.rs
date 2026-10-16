//! A MaterialX document built in memory, element by element, for hosts that hold their own graph (a USD UsdShade
//! network) and would otherwise write XML only to have it parsed again. Values are typed, every element may carry
//! the host's own path for it (`source_path`), which messages show in place of a file and line, and each element is
//! checked as it lands, so a mistake is reported at the call that made it.

use std::sync::atomic::{AtomicU64, Ordering};

use shadewire::{Error, Location, SourceInfo, Type};

use super::{Document, Input, Node, NodeGraph, Output, PortDef, PortType, Site, UiAttrs};
use crate::elaborate::elaborate;
use crate::index::Index;
use crate::library::standard;
use crate::{IrOptions, MaterialIr};

/// The id of the next builder. 0 is no builder: the id `GraphRef::default()` carries.
static NEXT_BUILDER: AtomicU64 = AtomicU64::new(1);

/// A nodegraph of a builder. Handles carry their builder's id, so a handle given to another builder is refused
/// rather than naming whatever element sits at the same index there. `GraphRef::default()` belongs to no builder:
/// it is the first nodegraph of whichever builder it is given to, a zeroed handle a C host can write.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GraphRef {
    builder: u64,
    index: usize,
}

/// A node of a builder: in the document (`graph` is `None`) or in a nodegraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeRef {
    builder: u64,
    graph: Option<usize>,
    index: usize,
}

/// A typed value, written as MaterialX text: floats for every float-based type (`color3` takes three), integers
/// and booleans for their own types, text for `string` and `filename`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputValue<'v> {
    Floats(&'v [f32]),
    Integer(i32),
    Boolean(bool),
    Text(&'v str),
}

/// What an input or an output reads: a node of the same scope (by output name when it has several), an output of
/// a nodegraph, or an interface input of the enclosing nodegraph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source<'s> {
    Node(NodeRef, Option<&'s str>),
    Graph(GraphRef, &'s str),
    Interface(&'s str),
}

/// The connection attributes a `Source` becomes.
#[derive(Default)]
struct Link {
    nodename: Option<String>,
    nodegraph: Option<String>,
    output: Option<String>,
    interfacename: Option<String>,
}

pub struct DocumentBuilder {
    id: u64,
    doc: Document,
}

impl Default for DocumentBuilder {
    fn default() -> DocumentBuilder {
        DocumentBuilder::new()
    }
}

fn site(source: Option<&str>) -> Site<'_> {
    Site { line: 0, source }
}

/// An input with no value and no connection yet.
fn blank_input(name: &str, port_type: PortType, source: Option<&str>) -> Input {
    Input {
        name: name.to_string(),
        port_type,
        value: None,
        nodename: None,
        nodegraph: None,
        output: None,
        interfacename: None,
        channels: None,
        colorspace: None,
        unit: None,
        unittype: None,
        ui: UiAttrs::default(),
        line: 0,
        source: source.map(str::to_string),
    }
}

/// The kind of a value, for messages.
fn kind(value: &InputValue) -> &'static str {
    match value {
        InputValue::Floats(_) => "floats",
        InputValue::Integer(_) => "integer",
        InputValue::Boolean(_) => "boolean",
        InputValue::Text(_) => "text",
    }
}

impl DocumentBuilder {
    /// An empty MaterialX 1.39 document; nodedefs are looked up in the standard library.
    pub fn new() -> DocumentBuilder {
        DocumentBuilder { id: NEXT_BUILDER.fetch_add(1, Ordering::Relaxed), doc: Document::empty() }
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn add_nodegraph(&mut self, name: &str, source: Option<&str>) -> Result<GraphRef, Error> {
        self.push_nodegraph(NodeGraph {
            name: name.to_string(),
            nodedef: None,
            interface: Vec::new(),
            nodes: Vec::new(),
            outputs: Vec::new(),
            line: 0,
            source: source.map(str::to_string),
        })
    }

    pub fn add_interface_input(
        &mut self,
        g: GraphRef,
        name: &str,
        ty: &str,
        value: Option<InputValue>,
        source: Option<&str>,
    ) -> Result<(), Error> {
        let graph = self.graph(g, site(source))?;
        let path = format!("{}/{name}", self.doc.nodegraphs[graph].name);
        let port_type = PortType::parse(ty);
        let value = value.map(|v| self.text(v, &port_type, site(source), &path)).transpose()?;
        self.push_interface(
            graph,
            PortDef {
                name: name.to_string(),
                port_type,
                value,
                default_geomprop: None,
                nodename: None,
                uniform: false,
                colorspace: None,
                unit: None,
                unittype: None,
                enum_values: None,
                ui: UiAttrs::default(),
                line: 0,
                source: source.map(str::to_string),
            },
        )
    }

    /// The colorspace of an interface input. A type without one (a float) drops it, as the XML reader does.
    pub fn set_interface_colorspace(&mut self, g: GraphRef, name: &str, colorspace: &str) -> Result<(), Error> {
        let graph = self.graph(g, Site::NONE)?;
        let path = format!("{}/{name}", self.doc.nodegraphs[graph].name);
        let Some(port) = self.doc.nodegraphs[graph].interface.iter().position(|p| p.name == name) else {
            return Err(self.bad(Site::NONE, path, format!("no interface input {name}")));
        };
        let port = &mut self.doc.nodegraphs[graph].interface[port];
        if port.port_type.has_colorspace() {
            port.colorspace = Some(colorspace.to_string());
        }
        Ok(())
    }

    /// A node of `category` whose output type is `ty`; its nodedef is picked at elaboration, as for XML.
    pub fn add_node(
        &mut self,
        g: Option<GraphRef>,
        category: &str,
        ty: &str,
        name: &str,
        source: Option<&str>,
    ) -> Result<NodeRef, Error> {
        let mut node = blank_node(name, source);
        node.category = category.to_string();
        node.port_type = PortType::parse(ty);
        self.push_node(g, node)
    }

    /// A node of a nodedef, whose category and output type come from it: a nodedef of several outputs makes a
    /// `multioutput` node. Inputs set with no type then take theirs from the nodedef.
    pub fn add_node_from_nodedef(
        &mut self,
        g: Option<GraphRef>,
        nodedef: &str,
        name: &str,
        source: Option<&str>,
    ) -> Result<NodeRef, Error> {
        let graph = g.map(|g| self.graph(g, site(source))).transpose()?;
        let found = Index::new(&self.doc, standard()).nodedef(nodedef).map(|nd| {
            let port_type = match nd.outputs.as_slice() {
                [one] => one.port_type.clone(),
                [] => PortType::Other(String::new()),
                _ => PortType::MultiOutput,
            };
            (nd.node.clone(), port_type)
        });
        let Some((category, port_type)) = found else {
            let path = self.path(graph, name);
            return Err(self.bad(site(source), path, format!("no nodedef named {nodedef}")));
        };
        let mut node = blank_node(name, source);
        node.category = category;
        node.port_type = port_type;
        node.nodedef = Some(nodedef.to_string());
        self.push_node(g, node)
    }

    /// Sets `input` to `value`. `ty: None` takes the input's type from the node's nodedef.
    pub fn set_input(
        &mut self,
        n: NodeRef,
        input: &str,
        ty: Option<&str>,
        value: InputValue,
        source: Option<&str>,
    ) -> Result<(), Error> {
        let at = site(source);
        self.check_node(n, at)?;
        let path = format!("{}.{input}", self.node_path(n));
        let port_type = self.input_type(n, input, ty, at, &path)?;
        let text = self.text(value, &port_type, at, &path)?;
        let mut new = blank_input(input, port_type, source);
        new.value = Some(text);
        self.push_input(n, new, &path)
    }

    /// The colorspace of an input already set or connected, kept whatever its type.
    pub fn set_input_colorspace(&mut self, n: NodeRef, input: &str, colorspace: &str) -> Result<(), Error> {
        self.check_node(n, Site::NONE)?;
        let path = format!("{}.{input}", self.node_path(n));
        let at = self.node(n).site();
        let Some(found) = self.node(n).inputs.iter().position(|i| i.name == input) else {
            return Err(self.bad(at, path, format!("no input {input} was set")));
        };
        self.doc_nodes_mut(n.graph)[n.index].inputs[found].colorspace = Some(colorspace.to_string());
        Ok(())
    }

    /// Connects `input` to `from`. `ty: None` takes the input's type from the node's nodedef. A nodegraph's
    /// outputs are looked up at elaboration, so they may be added after the connections that read them.
    pub fn connect(
        &mut self,
        n: NodeRef,
        input: &str,
        ty: Option<&str>,
        from: Source,
        source: Option<&str>,
    ) -> Result<(), Error> {
        let at = site(source);
        self.check_node(n, at)?;
        let path = format!("{}.{input}", self.node_path(n));
        let port_type = self.input_type(n, input, ty, at, &path)?;
        let link = self.link(n.graph, from, at, &path)?;
        let mut new = blank_input(input, port_type, source);
        (new.nodename, new.nodegraph, new.output, new.interfacename) =
            (link.nodename, link.nodegraph, link.output, link.interfacename);
        self.push_input(n, new, &path)
    }

    /// An output of a nodegraph, or of the document when `g` is `None`.
    pub fn add_output(
        &mut self,
        g: Option<GraphRef>,
        name: &str,
        ty: &str,
        from: Source,
        source: Option<&str>,
    ) -> Result<(), Error> {
        let at = site(source);
        let graph = g.map(|g| self.graph(g, at)).transpose()?;
        let path = self.path(graph, name);
        let link = self.link(graph, from, at, &path)?;
        self.push_output(
            g,
            Output {
                name: name.to_string(),
                port_type: PortType::parse(ty),
                nodename: link.nodename,
                nodegraph: link.nodegraph,
                output: link.output,
                interfacename: link.interfacename,
                line: 0,
                source: source.map(str::to_string),
            },
        )
    }

    /// A `surfacematerial` reading `surface`, which must be a surfaceshader node of the document itself, as
    /// elaboration finds materials only there.
    pub fn add_material(&mut self, name: &str, surface: NodeRef, source: Option<&str>) -> Result<(), Error> {
        let at = site(source);
        self.check_node(surface, at)?;
        let shader = self.node(surface);
        if surface.graph.is_some() || shader.port_type != PortType::SurfaceShader {
            let message = format!("{} is not a document-level surfaceshader node", self.node_path(surface));
            return Err(self.bad(at, name.to_string(), message));
        }
        let mut input = blank_input("surfaceshader", PortType::SurfaceShader, None);
        input.nodename = Some(shader.name.clone());
        let mut node = blank_node(name, source);
        node.category = "surfacematerial".to_string();
        node.port_type = PortType::Material;
        node.inputs.push(input);
        self.push_node(None, node).map(|_| ())
    }

    /// Lowers `material` (or the only one) to the IR, as the MaterialX front end does a parsed document. The
    /// builder is left as it was, so it may be edited and lowered again.
    pub fn to_ir(&self, material: Option<&str>, options: &IrOptions) -> Result<MaterialIr, Error> {
        let dag = elaborate(&self.doc, standard(), options, material)?;
        let (major, minor) = self.doc.version;
        let source = SourceInfo { source: "mtlx".to_string(), mtlx_version: format!("{major}.{minor}") };
        Ok(MaterialIr { dag, source, warnings: Vec::new() })
    }

    /// Adds a node to the document or to a nodegraph. Every insertion, typed or read from XML, goes through here
    /// and its siblings, so names are checked once: unique in their scope, and the node's inputs unique.
    pub(crate) fn push_node(&mut self, g: Option<GraphRef>, node: Node) -> Result<NodeRef, Error> {
        let graph = g.map(|g| self.graph(g, node.site())).transpose()?;
        let path = self.path(graph, &node.name);
        self.check_name(graph, &node.name, node.site(), &path)?;
        for (i, input) in node.inputs.iter().enumerate() {
            if input.name.is_empty() || node.inputs[..i].iter().any(|other| other.name == input.name) {
                let message = format!("input {:?} is empty or defined twice", input.name);
                return Err(self.bad(input.site(), format!("{path}.{}", input.name), message));
            }
        }
        let builder = self.id;
        let nodes = self.doc_nodes_mut(graph);
        nodes.push(node);
        Ok(NodeRef { builder, graph, index: nodes.len() - 1 })
    }

    /// Adds a nodegraph, then its interface, nodes and outputs one by one through the same checks; on an error
    /// the graph is taken back out.
    pub(crate) fn push_nodegraph(&mut self, mut graph: NodeGraph) -> Result<GraphRef, Error> {
        self.check_name(None, &graph.name, graph.site(), &graph.name)?;
        let interface = std::mem::take(&mut graph.interface);
        let nodes = std::mem::take(&mut graph.nodes);
        let outputs = std::mem::take(&mut graph.outputs);
        self.doc.nodegraphs.push(graph);
        let g = GraphRef { builder: self.id, index: self.doc.nodegraphs.len() - 1 };
        let filled = (|| {
            for port in interface {
                self.push_interface(g.index, port)?;
            }
            for node in nodes {
                self.push_node(Some(g), node)?;
            }
            outputs.into_iter().try_for_each(|output| self.push_output(Some(g), output))
        })();
        match filled {
            Ok(()) => Ok(g),
            Err(err) => {
                self.doc.nodegraphs.pop();
                Err(err)
            }
        }
    }

    /// Adds an output to a nodegraph, or to the document when `g` is `None`.
    pub(crate) fn push_output(&mut self, g: Option<GraphRef>, output: Output) -> Result<(), Error> {
        let graph = g.map(|g| self.graph(g, output.site())).transpose()?;
        let path = self.path(graph, &output.name);
        self.check_name(graph, &output.name, output.site(), &path)?;
        match graph {
            Some(graph) => self.doc.nodegraphs[graph].outputs.push(output),
            None => self.doc.outputs.push(output),
        }
        Ok(())
    }

    fn push_interface(&mut self, graph: usize, port: PortDef) -> Result<(), Error> {
        let path = format!("{}/{}", self.doc.nodegraphs[graph].name, port.name);
        self.check_name(Some(graph), &port.name, port.site(), &path)?;
        self.doc.nodegraphs[graph].interface.push(port);
        Ok(())
    }

    /// An input is set or connected once; a second time is a mistake of the host, not an edit.
    fn push_input(&mut self, n: NodeRef, input: Input, path: &str) -> Result<(), Error> {
        if input.name.is_empty() || self.node(n).inputs.iter().any(|i| i.name == input.name) {
            let message = format!("input {:?} is empty or defined twice", input.name);
            return Err(self.bad(input.site(), path.to_string(), message));
        }
        self.doc_nodes_mut(n.graph)[n.index].inputs.push(input);
        Ok(())
    }

    /// The children of a scope share one namespace, as in MaterialX: the document's nodes, nodegraphs and
    /// outputs; a nodegraph's interface inputs, nodes and outputs.
    fn check_name(&self, graph: Option<usize>, name: &str, at: Site<'_>, path: &str) -> Result<(), Error> {
        if name.is_empty() {
            return Err(self.bad(at, path.to_string(), "an element needs a name".to_string()));
        }
        let taken = match graph {
            None => {
                self.doc.nodes.iter().any(|n| n.name == name)
                    || self.doc.nodegraphs.iter().any(|g| g.name == name)
                    || self.doc.outputs.iter().any(|o| o.name == name)
            }
            Some(graph) => {
                let graph = &self.doc.nodegraphs[graph];
                graph.interface.iter().any(|p| p.name == name)
                    || graph.nodes.iter().any(|n| n.name == name)
                    || graph.outputs.iter().any(|o| o.name == name)
            }
        };
        if taken {
            return Err(self.bad(at, path.to_string(), format!("{name} is defined twice")));
        }
        Ok(())
    }

    /// The nodegraph a handle names. A default handle names the first nodegraph.
    fn graph(&self, g: GraphRef, at: Site<'_>) -> Result<usize, Error> {
        if (g.builder == self.id || g.builder == 0) && g.index < self.doc.nodegraphs.len() {
            return Ok(g.index);
        }
        Err(self.bad(at, String::new(), "not a nodegraph of this builder".to_string()))
    }

    fn check_node(&self, n: NodeRef, at: Site<'_>) -> Result<(), Error> {
        let scope_ok = n.graph.is_none_or(|g| g < self.doc.nodegraphs.len());
        if n.builder == self.id && scope_ok && n.index < self.doc_nodes(n.graph).len() {
            return Ok(());
        }
        Err(self.bad(at, String::new(), "not a node of this builder".to_string()))
    }

    fn doc_nodes(&self, graph: Option<usize>) -> &Vec<Node> {
        match graph {
            Some(g) => &self.doc.nodegraphs[g].nodes,
            None => &self.doc.nodes,
        }
    }

    fn doc_nodes_mut(&mut self, graph: Option<usize>) -> &mut Vec<Node> {
        match graph {
            Some(g) => &mut self.doc.nodegraphs[g].nodes,
            None => &mut self.doc.nodes,
        }
    }

    fn node(&self, n: NodeRef) -> &Node {
        &self.doc_nodes(n.graph)[n.index]
    }

    /// An element's path in messages, as the elaborator spells it: `graph/name`, or `name` in the document.
    fn path(&self, graph: Option<usize>, name: &str) -> String {
        match graph {
            Some(g) => format!("{}/{name}", self.doc.nodegraphs[g].name),
            None => name.to_string(),
        }
    }

    fn node_path(&self, n: NodeRef) -> String {
        self.path(n.graph, &self.node(n).name)
    }

    fn bad(&self, at: Site<'_>, path: String, message: String) -> Error {
        Error::BadValue { at: self.location(at, path), message }
    }

    fn location(&self, at: Site<'_>, path: String) -> Location {
        Location { file: self.doc.file.clone(), line: at.line, path, source: at.source.map(str::to_string) }
    }

    /// The type an input is declared with: `ty`, or with none, the type its node's nodedef gives it.
    fn input_type(
        &self,
        n: NodeRef,
        input: &str,
        ty: Option<&str>,
        at: Site<'_>,
        path: &str,
    ) -> Result<PortType, Error> {
        if let Some(ty) = ty {
            return Ok(PortType::parse(ty));
        }
        let index = Index::new(&self.doc, standard());
        let port = self.node(n).nodedef.as_deref().and_then(|nd| index.nodedef(nd));
        match port.and_then(|nd| nd.inputs.iter().find(|p| p.name == input)) {
            Some(port) => Ok(port.port_type.clone()),
            None => Err(self.bad(at, path.to_string(), format!("no type given for {input} and no nodedef has it"))),
        }
    }

    /// `value` as MaterialX text for `port_type`. Floats print in Rust's shortest form that reads back as the
    /// same f32, so the document holds exactly the value the host gave.
    fn text(&self, value: InputValue, port_type: &PortType, at: Site<'_>, path: &str) -> Result<String, Error> {
        let mismatch = || Error::TypeMismatch {
            at: self.location(at, path.to_string()),
            expected: port_type.mtlx_name(),
            found: kind(&value).to_string(),
        };
        match (port_type, value) {
            (PortType::Value(Type::Boolean), InputValue::Boolean(b)) => Ok(b.to_string()),
            (PortType::Value(Type::Integer), InputValue::Integer(i)) => Ok(i.to_string()),
            (PortType::Value(Type::Boolean | Type::Integer), _) => Err(mismatch()),
            (PortType::Value(ty), InputValue::Floats(floats)) => {
                if floats.len() != ty.component_count() {
                    let message =
                        format!("{} takes {} values, given {}", ty.mtlx_name(), ty.component_count(), floats.len());
                    return Err(self.bad(at, path.to_string(), message));
                }
                if let Some(bad) = floats.iter().find(|f| !f.is_finite()) {
                    return Err(self.bad(at, path.to_string(), format!("{bad} is not a finite value")));
                }
                Ok(floats.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(", "))
            }
            (PortType::String | PortType::Filename, InputValue::Text(text)) => Ok(text.to_string()),
            (PortType::Other(ty), _) => {
                Err(Error::UnsupportedType { at: self.location(at, path.to_string()), ty: ty.clone() })
            }
            _ => Err(mismatch()),
        }
    }

    /// The connection `from` makes for an element of scope `graph`: a node of the same scope (by an output its
    /// nodedef has, when it has several), a nodegraph's output, or the enclosing nodegraph's interface input.
    fn link(&self, graph: Option<usize>, from: Source, at: Site<'_>, path: &str) -> Result<Link, Error> {
        match from {
            Source::Node(m, output) => {
                self.check_node(m, at)?;
                let node = self.node(m);
                if m.graph != graph {
                    let message = format!("{} is not in the scope of {path}", self.node_path(m));
                    return Err(self.bad(at, path.to_string(), message));
                }
                if let (Some(output), PortType::MultiOutput) = (output, &node.port_type) {
                    let index = Index::new(&self.doc, standard());
                    let nodedef = node.nodedef.as_deref().and_then(|nd| index.nodedef(nd));
                    if nodedef.is_some_and(|nd| !nd.outputs.iter().any(|o| o.name == output)) {
                        let message = format!("{} has no output {output}", node.name);
                        return Err(self.bad(at, path.to_string(), message));
                    }
                }
                let output = output.map(str::to_string);
                Ok(Link { nodename: Some(node.name.clone()), output, ..Link::default() })
            }
            Source::Graph(g, output) => {
                let g = self.graph(g, at)?;
                let nodegraph = Some(self.doc.nodegraphs[g].name.clone());
                Ok(Link { nodegraph, output: Some(output.to_string()), ..Link::default() })
            }
            Source::Interface(name) => {
                let declared = graph.is_some_and(|g| self.doc.nodegraphs[g].interface.iter().any(|p| p.name == name));
                if !declared {
                    let message = format!("no interface input {name} in the enclosing nodegraph");
                    return Err(self.bad(at, path.to_string(), message));
                }
                Ok(Link { interfacename: Some(name.to_string()), ..Link::default() })
            }
        }
    }
}

fn blank_node(name: &str, source: Option<&str>) -> Node {
    Node {
        category: String::new(),
        name: name.to_string(),
        port_type: PortType::Other(String::new()),
        nodedef: None,
        version: None,
        inputs: Vec::new(),
        line: 0,
        source: source.map(str::to_string),
    }
}
