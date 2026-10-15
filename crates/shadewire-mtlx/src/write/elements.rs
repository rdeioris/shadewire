//! The element builders of the pattern nodegraph, one per kind of node.

use std::collections::BTreeMap;

use shadewire::{AddressMode, ContextRead, FilterMode, TextureDesc, Type, UiHints, Value};
use shadewire_nodes::document::{NodeDef, PortType};

use super::catalog::Category;
use super::values::value_text;
use super::xml::Element;

/// What an input of a written node reads.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Another node of the graph, and the output read when that node has several.
    Node { name: String, output: Option<String> },
    /// A nodegraph input.
    Interface(String),
    /// A value written on the input itself.
    Value(String),
}

/// `<input name type .../>` reading `source`.
pub fn input(name: &str, ty: &str, source: &Source) -> Element {
    let element = Element::new("input").attr("name", name).attr("type", ty);
    match source {
        Source::Node { name, output: Some(output) } => element.attr("nodename", name).attr("output", output),
        Source::Node { name, output: None } => element.attr("nodename", name),
        Source::Interface(name) => element.attr("interfacename", name),
        Source::Value(text) => element.attr("value", text),
    }
}

/// `<input name type value/>`.
fn value_input(name: &str, ty: &str, value: impl Into<String>) -> Element {
    input(name, ty, &Source::Value(value.into()))
}

/// A nodegraph input for a uniform, with its ui hints.
pub fn uniform_element(name: &str, ty: Type, value: &Value, ui: Option<&UiHints>) -> Element {
    let mut element = value_input(name, ty.mtlx_name(), value_text(value));
    let Some(ui) = ui else { return element };
    let limits = [("uimin", &ui.min), ("uimax", &ui.max), ("uisoftmin", &ui.soft_min), ("uisoftmax", &ui.soft_max)];
    for (key, limit) in limits {
        if let Some(limit) = limit {
            element = element.attr(key, value_text(limit));
        }
    }
    if let Some(folder) = &ui.folder {
        element = element.attr("uifolder", folder);
    }
    element
}

/// `<constant name type><input name="value" .../></constant>`.
pub fn constant_element(name: &str, ty: Type, value: &Value) -> Element {
    let ty = ty.mtlx_name();
    Element::new("constant").attr("name", name).attr("type", ty).child(value_input("value", ty, value_text(value)))
}

/// `<dot name type>` reading a nodegraph input: how a uniform reaches a nodegraph output.
pub fn dot_element(name: &str, ty: Type, interface: &str) -> Element {
    let ty = ty.mtlx_name();
    Element::new("dot").attr("name", name).attr("type", ty).child(input("in", ty, &Source::Interface(interface.into())))
}

/// The geometric node of a host read, with its `space`, `index` or `geomprop` input.
pub fn context_element(name: &str, read: &ContextRead, geomprop_defaults: &BTreeMap<String, Value>) -> Element {
    let inputs = match read {
        ContextRead::Position(s)
        | ContextRead::Normal(s)
        | ContextRead::Tangent(s)
        | ContextRead::Bitangent(s)
        | ContextRead::ViewDirection(s) => vec![value_input("space", "string", s.name())],
        ContextRead::TexCoord { index, .. } | ContextRead::GeomColor { index, .. } => {
            vec![value_input("index", "integer", index.to_string())]
        }
        ContextRead::GeomProp { name: prop, ty } => {
            let mut inputs = vec![value_input("geomprop", "string", prop.as_str())];
            if let Some(default) = geomprop_defaults.get(prop) {
                inputs.push(value_input("default", ty.mtlx_name(), value_text(default)));
            }
            inputs
        }
        ContextRead::Time | ContextRead::Frame => Vec::new(),
    };
    let mut element = Element::new(context_category(read)).attr("name", name).attr("type", read.ty().mtlx_name());
    element.children = inputs;
    element
}

/// The node category of a host read.
pub fn context_category(read: &ContextRead) -> &'static str {
    match read {
        ContextRead::Position(_) => "position",
        ContextRead::Normal(_) => "normal",
        ContextRead::Tangent(_) => "tangent",
        ContextRead::Bitangent(_) => "bitangent",
        ContextRead::ViewDirection(_) => "viewdirection",
        ContextRead::TexCoord { .. } => "texcoord",
        ContextRead::GeomColor { .. } => "geomcolor",
        ContextRead::GeomProp { .. } => "geompropvalue",
        ContextRead::Time => "time",
        ContextRead::Frame => "frame",
    }
}

/// An `image` (or, with gradients, `shadewire_image_grad`) node sampling `texture`.
pub fn image_element(
    category: &str,
    name: &str,
    ty: Type,
    texture: &TextureDesc,
    default: &Value,
    uv: &Source,
    grad: Option<[Source; 2]>,
) -> Element {
    let mut file = value_input("file", "filename", texture.file.as_str());
    if let Some(colorspace) = &texture.colorspace {
        file = file.attr("colorspace", colorspace);
    }
    let mut element = Element::new(category).attr("name", name).attr("type", ty.mtlx_name()).child(file);
    element.push(input("texcoord", "vector2", uv));
    if let Some([dx, dy]) = &grad {
        element.push(input("dx", "vector2", dx));
        element.push(input("dy", "vector2", dy));
    }
    for (input_name, mode) in [("uaddressmode", texture.address_u), ("vaddressmode", texture.address_v)] {
        if mode != AddressMode::Periodic {
            element.push(value_input(input_name, "string", address_name(mode)));
        }
    }
    if texture.filter != FilterMode::Linear {
        element.push(value_input("filtertype", "string", filter_name(texture.filter)));
    }
    element.child(value_input("default", ty.mtlx_name(), value_text(default)))
}

fn address_name(mode: AddressMode) -> &'static str {
    match mode {
        AddressMode::Constant => "constant",
        AddressMode::Clamp => "clamp",
        AddressMode::Periodic => "periodic",
        AddressMode::Mirror => "mirror",
    }
}

fn filter_name(filter: FilterMode) -> &'static str {
    match filter {
        FilterMode::Closest => "closest",
        FilterMode::Linear => "linear",
        FilterMode::Cubic => "cubic",
    }
}

/// The type of nodedef input `name`, as MaterialX spells it.
fn port_type(nd: &NodeDef, name: &str) -> Option<String> {
    nd.inputs.iter().find(|p| p.name == name).map(|p| p.port_type.mtlx_name())
}

/// An op's node: one input per argument (typed by the nodedef's port), then the op's parameters; `nodedef=` only
/// for Shadewire's own categories.
pub fn op_element(name: &str, category: &Category, nd: &NodeDef, ty: Type, args: &[(&str, Source)]) -> Element {
    let out = match nd.outputs.first().map(|o| &o.port_type) {
        Some(PortType::Value(t)) => t.mtlx_name(),
        _ => ty.mtlx_name(),
    };
    let mut element = Element::new(&category.name).attr("name", name).attr("type", out);
    if super::catalog::is_shadewire_category(&category.name) {
        element = element.attr("nodedef", &nd.name);
    }
    for (input_name, source) in args {
        let input_type = port_type(nd, input_name).unwrap_or_else(|| ty.mtlx_name().to_string());
        element.push(input(input_name, &input_type, source));
    }
    for (input_name, text, input_type) in &category.params {
        element.push(value_input(input_name, input_type, text.as_str()));
    }
    element
}

/// A library node written as one node: `type` is its output's type, or `multioutput`.
pub fn compound_element(name: &str, nd: &NodeDef, inputs: &[(&str, Source)]) -> Element {
    let ty = match nd.outputs.as_slice() {
        [only] => only.port_type.mtlx_name(),
        _ => "multioutput".to_string(),
    };
    let mut element = Element::new(&nd.node).attr("name", name).attr("type", ty).attr("nodedef", &nd.name);
    for (input_name, source) in inputs {
        let input_type = port_type(nd, input_name).unwrap_or_default();
        element.push(input(input_name, &input_type, source));
    }
    element
}

/// `<output name type nodename [output]/>`.
pub fn output_element(name: &str, ty: Type, source: &Source) -> Element {
    let mut element = input(name, ty.mtlx_name(), source);
    element.tag = "output".to_string();
    element
}
