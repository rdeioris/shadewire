//! The IR as a MaterialX document.

pub mod catalog;
pub mod elements;
pub mod emit;
pub mod names;
pub mod patterns;
pub mod plan;
pub mod reach;
pub mod readers;
pub mod shader;
pub mod surface;
pub mod values;
pub mod xml;

use shadewire::{Dag, Error};
use shadewire_nodes::Index;
use shadewire_nodes::document::empty_document;
use shadewire_nodes::library::standard;

use catalog::is_shadewire_category;
use elements::{Source, input};
use names::Names;
use patterns::write_patterns;
use plan::plan;
use surface::write_surface;
use xml::{Element, to_text};

/// How `from_ir` writes a document.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteOptions {
    /// Write every library node expanded into the nodes it was built from, instead of as one node.
    pub inline: bool,
    /// Embed the declarations of Shadewire's own nodedefs in documents that use them.
    pub embed_shadewire_defs: bool,
    /// The material's name; the nodegraph and shader names derive from it.
    pub name: String,
}

impl Default for WriteOptions {
    fn default() -> WriteOptions {
        WriteOptions { inline: false, embed_shadewire_defs: true, name: "material".to_string() }
    }
}

/// A written document, and what it loses or needs.
#[derive(Clone, Debug, PartialEq)]
pub struct Written {
    /// The MaterialX 1.39 document.
    pub text: String,
    /// Compounds written expanded, Shadewire nodedefs the document uses.
    pub warnings: Vec<String>,
}

/// The dag as a MaterialX document: the nodegraph `NG_{name}` (when it has any element), the shader level, and
/// the `surfacematerial` `{name}`. Takes an unoptimized dag (as `to_ir` returns it): its compounds are what
/// library nodes and colorspace conversions are written from.
pub fn from_ir(dag: &Dag, options: &WriteOptions) -> Result<Written, Error> {
    let index = Index::new(empty_document(), standard());
    let plan = plan(dag, &index, options.inline);
    let mut names = Names::new();
    names.reserve(&options.name);
    let mut patterns = write_patterns(dag, &plan, &index, options, &mut names)?;
    let shader_name = names.unique(&format!("{}_shader", options.name));
    let shader_level = write_surface(dag, &index, &mut patterns, &mut names, &shader_name)?;
    let mut document = Element::new("materialx").attr("version", "1.39");
    let graph: Vec<Element> = patterns.inputs.iter().chain(&patterns.nodes).chain(&patterns.outputs).cloned().collect();
    let uses_shadewire = graph.iter().any(|e| is_shadewire_category(&e.tag));
    if !graph.is_empty() {
        let mut nodegraph = Element::new("nodegraph").attr("name", &patterns.graph_name);
        nodegraph.children = graph;
        document.push(nodegraph);
    }
    document.children.extend(shader_level);
    let shader = input("surfaceshader", "surfaceshader", &Source::Node { name: shader_name, output: None });
    document.push(Element::new("surfacematerial").attr("name", &options.name).attr("type", "material").child(shader));
    if options.embed_shadewire_defs && uses_shadewire {
        document.children.extend(shadewire_declarations());
    }
    Ok(Written { text: to_text(&document), warnings: patterns.warnings })
}

/// The declarations of Shadewire's own nodedefs, for a document that uses them; none are declared yet.
fn shadewire_declarations() -> Vec<Element> {
    Vec::new()
}
