//! The shader level as elements: each closure node and the shader, their value inputs reached through the
//! nodegraph.

use shadewire::{Dag, Error};
use shadewire_nodes::Index;

use super::elements::{Source, input, with_ui_hints};
use super::names::Names;
use super::patterns::{Patterns, Reach};
use super::shader::{Read, shader_nodes};
use super::xml::Element;

/// The shader-level elements of `dag`, the shader (named `shader_name`) last. Closure nodes are named
/// `{material}_{category}_{position}`, so documents of several materials can be merged; each value input is a
/// value or an output of the nodegraph `patterns` writes.
pub fn write_surface(
    dag: &Dag,
    index: &Index,
    patterns: &mut Patterns<'_>,
    names: &mut Names,
    material: &str,
    shader_name: &str,
) -> Result<Vec<Element>, Error> {
    let nodes = shader_nodes(dag, index)?;
    let mut element_names: Vec<String> = Vec::new();
    let mut elements = Vec::new();
    for (position, node) in nodes.iter().enumerate() {
        let name = match position + 1 == nodes.len() {
            true => shader_name.to_string(),
            false => names.unique(&format!("{material}_{}_{position}", node.category)),
        };
        let mut element = Element::new(&node.category).attr("name", &name).attr("type", &node.ty);
        for (input_name, ty, read) in &node.inputs {
            let child = match read {
                Read::Field { node, value_allowed } => match patterns.reach(*node, input_name, *value_allowed, dag)? {
                    // A value is a uniform named after the input: its ui hints go on the input.
                    Reach::Value(text) => {
                        with_ui_hints(input(input_name, ty, &Source::Value(text)), dag.uniform_ui.get(input_name))
                    }
                    Reach::Output(output) => Element::new("input")
                        .attr("name", input_name)
                        .attr("type", ty)
                        .attr("nodegraph", &patterns.graph_name)
                        .attr("output", output),
                },
                Read::Text(text) => input(input_name, ty, &Source::Value(text.clone())),
                Read::Closure(child) => {
                    input(input_name, ty, &Source::Node { name: element_names[*child].clone(), output: None })
                }
            };
            element.push(child);
        }
        elements.push(element);
        element_names.push(name);
    }
    Ok(elements)
}
