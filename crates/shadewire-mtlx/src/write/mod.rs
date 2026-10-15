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
pub mod shadewire_defs;
pub mod surface;
pub mod values;
pub mod xml;

use shadewire::{Dag, Error};
use shadewire_nodes::Index;
use shadewire_nodes::document::empty_document;
use shadewire_nodes::library::standard;

use catalog::is_shadewire_category;
use elements::{Source, input};
use names::{Names, sanitize_name};
use patterns::write_patterns;
use plan::plan;
use shadewire_defs::shadewire_declarations;
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
    let name = sanitize_name(&options.name);
    names.reserve(&name);
    let mut patterns = write_patterns(dag, &plan, &index, options, &mut names)?;
    let shader_name = names.unique(&format!("{name}_shader"));
    let shader_level = write_surface(dag, &index, &mut patterns, &mut names, &shader_name)?;
    let mut document = Element::new("materialx").attr("version", "1.39");
    let graph: Vec<Element> = patterns.inputs.iter().chain(&patterns.nodes).chain(&patterns.outputs).cloned().collect();
    let mut used: Vec<&str> = Vec::new();
    for tag in graph.iter().map(|e| e.tag.as_str()).filter(|tag| is_shadewire_category(tag)) {
        if !used.contains(&tag) {
            used.push(tag);
        }
    }
    let declarations = if options.embed_shadewire_defs { shadewire_declarations(&used) } else { Vec::new() };
    if !graph.is_empty() {
        let mut nodegraph = Element::new("nodegraph").attr("name", &patterns.graph_name);
        nodegraph.children = graph;
        document.push(nodegraph);
    }
    document.children.extend(shader_level);
    let shader = input("surfaceshader", "surfaceshader", &Source::Node { name: shader_name, output: None });
    document.push(Element::new("surfacematerial").attr("name", &name).attr("type", "material").child(shader));
    document.children.extend(declarations);
    Ok(Written { text: to_text(&document), warnings: patterns.warnings })
}

#[cfg(test)]
mod tests {
    use super::{WriteOptions, from_ir};
    use crate::{IrOptions, Resolver};

    #[test]
    fn the_material_name_is_sanitized() {
        let text = r#"<materialx version="1.39">
          <surface_unlit name="s" type="surfaceshader"><input name="emission" type="float" value="0.5"/></surface_unlit>
          <surfacematerial name="m" type="material">
            <input name="surfaceshader" type="surfaceshader" nodename="s"/>
          </surfacematerial>
        </materialx>"#;
        let dag = crate::to_ir(text, None, None, &IrOptions::default(), &Resolver::default()).unwrap().dag;
        let written = from_ir(&dag, &WriteOptions { name: "my material/v2".into(), ..WriteOptions::default() });
        let written = written.unwrap().text;
        assert!(written.contains(r#"<surfacematerial name="my_material_v2""#), "{written}");
        assert!(written.contains(r#"name="my_material_v2_shader""#), "{written}");
    }
}
