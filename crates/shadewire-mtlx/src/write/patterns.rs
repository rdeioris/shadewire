//! The pattern side of a document: the dag's nodes as one nodegraph.

use std::collections::HashMap;

use shadewire::{Dag, Error, Node, NodeId, NodeKind};
use shadewire_nodes::Index;

use super::WriteOptions;
use super::elements::{constant_element, context_category, context_element, uniform_element};
use super::names::Names;
use super::plan::{Plan, shader_reads};
use super::xml::Element;

pub use super::reach::Reach;

/// The nodegraph's elements, and how the shader reaches into it.
pub struct Patterns<'a> {
    /// The nodegraph's name in the document.
    pub graph_name: String,
    /// Its `<input>`s: one per uniform the nodegraph reads or `reach` asks for, and one per unread uniform.
    pub inputs: Vec<Element>,
    /// Its nodes, in dag order, then the ones `reach` adds.
    pub nodes: Vec<Element>,
    /// Its `<output>`s, made by `reach`.
    pub outputs: Vec<Element>,
    /// The names used inside the nodegraph.
    pub names: Names,
    /// The element (node or nodegraph input) each written dag node is read from.
    pub node_names: HashMap<NodeId, String>,
    /// What the document loses or needs, for the caller to report.
    pub warnings: Vec<String>,
    /// The output port of a multi-output node each of its dag nodes is read by.
    pub(super) node_outputs: HashMap<NodeId, String>,
    /// Every uniform's name in the dag.
    pub(super) uniform_names: HashMap<NodeId, &'a str>,
    /// Nodegraph inputs not written yet: their uniforms are read by the shader only.
    pub(super) pending_uniforms: HashMap<NodeId, Element>,
    /// `Plan::readers`.
    pub(super) readers: HashMap<NodeId, usize>,
    /// `Plan::aliases`: interior nodes read as the compound output they compute again.
    pub(super) aliases: HashMap<NodeId, NodeId>,
    /// The output `reach` made for a node.
    pub(super) reached: HashMap<NodeId, String>,
}

/// Writes every node the plan does not skip, in dag order, into a nodegraph named `NG_{options.name}`.
pub fn write_patterns<'a>(
    dag: &'a Dag,
    plan: &Plan,
    index: &Index,
    options: &WriteOptions,
    names: &mut Names,
) -> Result<Patterns<'a>, Error> {
    let mut patterns = Patterns {
        graph_name: names.unique(&format!("NG_{}", options.name)),
        inputs: Vec::new(),
        nodes: Vec::new(),
        outputs: Vec::new(),
        names: Names::new(),
        node_names: HashMap::new(),
        warnings: plan.warnings.clone(),
        node_outputs: HashMap::new(),
        uniform_names: HashMap::new(),
        pending_uniforms: HashMap::new(),
        readers: plan.readers.clone(),
        aliases: plan.aliases.clone(),
        reached: HashMap::new(),
    };
    patterns.name_uniforms(dag);
    let mut shader_only: HashMap<NodeId, usize> = HashMap::new();
    for id in shader_reads(dag) {
        *shader_only.entry(id).or_insert(0) += 1;
    }
    // A kept compound is written where its first output was computed: after its inputs, before its readers.
    let mut compound_at: HashMap<NodeId, usize> = HashMap::new();
    for position in &plan.kept {
        if let Some(first) = dag.compounds[*position].outputs.iter().map(|(_, id)| *id).min() {
            compound_at.insert(first, *position);
        }
    }
    for (position, node) in dag.nodes.iter().enumerate() {
        let id = NodeId(position as u32);
        if let Some(compound) = compound_at.get(&id) {
            patterns.write_compound(dag, index, plan, *compound)?;
        }
        if plan.interior.contains(&id) || plan.inline_const.contains(&id) {
            continue;
        }
        // A const, context read or uniform read by the shader only is written when `reach` asks for it (the shader
        // may leave it at its default); an unread const or context read never.
        let readers = plan.readers.get(&id).copied().unwrap_or(0);
        let pattern_read = readers > shader_only.get(&id).copied().unwrap_or(0);
        match &node.kind {
            NodeKind::Const(_) | NodeKind::Context(_) if pattern_read => patterns.write_leaf(dag, id),
            NodeKind::Const(_) | NodeKind::Context(_) => {}
            NodeKind::Uniform { .. } if pattern_read || readers == 0 => patterns.write_uniform(id),
            NodeKind::Uniform { .. } => {}
            NodeKind::Sample { .. } => patterns.write_sample(dag, plan, id)?,
            NodeKind::Op { .. } => patterns.write_op(dag, plan, index, id)?,
        }
    }
    Ok(patterns)
}

impl<'a> Patterns<'a> {
    /// Reserves every uniform's name exactly and prepares its nodegraph input.
    fn name_uniforms(&mut self, dag: &'a Dag) {
        for (position, node) in dag.nodes.iter().enumerate() {
            let NodeKind::Uniform { name, value } = &node.kind else { continue };
            let id = NodeId(position as u32);
            let written = if self.names.reserve(name) { name.clone() } else { self.names.unique(name) };
            let element = uniform_element(&written, node.ty, value, dag.uniform_ui.get(name));
            self.pending_uniforms.insert(id, element);
            self.uniform_names.insert(id, name);
            self.node_names.insert(id, written);
        }
    }

    /// Writes a uniform's nodegraph input, once.
    pub(super) fn write_uniform(&mut self, id: NodeId) {
        if let Some(element) = self.pending_uniforms.remove(&id) {
            self.inputs.push(element);
        }
    }

    /// A `constant` node for a const, or the geometric node of a context read; once.
    pub(super) fn write_leaf(&mut self, dag: &Dag, id: NodeId) {
        let node = dag.node(id);
        if self.node_names.contains_key(&id) {
            return;
        }
        let (name, element) = match &node.kind {
            NodeKind::Const(value) => {
                let name = self.node_name(node, "constant", id.index());
                let element = constant_element(&name, node.ty, value);
                (name, element)
            }
            NodeKind::Context(read) => {
                let name = self.node_name(node, context_category(read), id.index());
                let element = context_element(&name, read, &dag.geomprop_defaults);
                (name, element)
            }
            _ => return,
        };
        self.nodes.push(element);
        self.node_names.insert(id, name);
    }

    /// A fresh name for a node: its origin, or `{category}_{index}` without one.
    pub(super) fn node_name(&mut self, node: &Node, category: &str, position: usize) -> String {
        match node.origin.as_str() {
            "" => self.names.unique(&format!("{category}_{position}")),
            origin => self.names.unique(origin),
        }
    }

    /// Warns, once per category, that the document uses a nodedef only Shadewire declares.
    pub(super) fn warn_shadewire(&mut self, category: &str) {
        let warning = format!("shadewire nodedef {category} written: the document reads only in Shadewire");
        if !self.warnings.contains(&warning) {
            self.warnings.push(warning);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::plan::plan;
    use super::super::xml::to_text;
    use super::*;
    use crate::{IrOptions, Resolver};
    use shadewire::*;
    use shadewire_nodes::Index;
    use shadewire_nodes::document::empty_document;
    use shadewire_nodes::library::standard;

    fn node(kind: NodeKind, ty: Type, origin: &str) -> Node {
        Node { kind, ty, origin: origin.to_string() }
    }

    fn written(dag: &Dag, inline: bool) -> (Patterns<'_>, String) {
        let index = Index::new(empty_document(), standard());
        let plan = plan(dag, &index, inline);
        let mut names = Names::new();
        let patterns = write_patterns(dag, &plan, &index, &WriteOptions::default(), &mut names).unwrap();
        let mut graph = Element::new("nodegraph").attr("name", &patterns.graph_name);
        for e in patterns.inputs.iter().chain(&patterns.nodes).chain(&patterns.outputs) {
            graph.push(e.clone());
        }
        let text = to_text(&graph);
        (patterns, text)
    }

    #[test]
    fn a_constant_read_once_by_an_op_is_inlined_and_a_shared_one_is_a_node() {
        let mut dag = Dag::new("surface_unlit");
        let two = dag.add(node(NodeKind::Const(Value::Float(2.0)), Type::Float, "g/two"));
        let shared = dag.add(node(NodeKind::Const(Value::Float(3.0)), Type::Float, "g/three"));
        let a = dag.add(node(NodeKind::Op { op: Op::Multiply, args: vec![two, shared] }, Type::Float, "g/a"));
        let b = dag.add(node(NodeKind::Op { op: Op::Add, args: vec![a, shared] }, Type::Float, "g/b"));
        dag.surface.fields.push(SurfaceField { name: "emission".into(), ty: Type::Float, node: b });
        let (_, text) = written(&dag, false);
        assert!(text.contains(r#"<multiply name="a" type="float">"#), "{text}");
        assert!(text.contains(r#"<input name="in1" type="float" value="2" />"#), "{text}");
        assert!(text.contains(r#"<constant name="three" type="float">"#), "{text}");
        assert!(text.contains(r#"<input name="in2" type="float" nodename="three" />"#), "{text}");
        assert!(!text.contains(r#"name="two""#), "{text}");
    }

    #[test]
    fn a_uniform_is_a_nodegraph_input_read_through_interfacename() {
        let mut dag = Dag::new("surface_unlit");
        let u = dag.add(node(NodeKind::Uniform { name: "gain".into(), value: Value::Float(0.5) }, Type::Float, "gain"));
        dag.uniform_ui.insert(
            "gain".into(),
            UiHints { min: Some(Value::Float(0.0)), max: Some(Value::Float(1.0)), ..UiHints::default() },
        );
        let s = dag.add(node(NodeKind::Op { op: Op::Sin, args: vec![u] }, Type::Float, "g/s"));
        dag.surface.fields.push(SurfaceField { name: "emission".into(), ty: Type::Float, node: s });
        let (_, text) = written(&dag, false);
        assert!(text.contains(r#"<input name="gain" type="float" value="0.5" uimax="1" uimin="0" />"#), "{text}");
        assert!(text.contains(r#"<input name="in" type="float" interfacename="gain" />"#), "{text}");
    }

    #[test]
    fn context_reads_image_and_a_folded_colorspace_conversion() {
        // to_ir of a document with an srgb image gives: texcoord, sample, then the cmlib conversion compound.
        let text = r#"<materialx version="1.39">
          <nodegraph name="g">
            <texcoord name="uv" type="vector2"/>
            <image name="img" type="color3">
              <input name="file" type="filename" value="a.png" colorspace="srgb_texture"/>
              <input name="texcoord" type="vector2" nodename="uv"/>
              <input name="uaddressmode" type="string" value="clamp"/>
            </image>
            <output name="out" type="color3" nodename="img"/>
          </nodegraph>
          <surface_unlit name="s" type="surfaceshader">
            <input name="emission_color" type="color3" nodegraph="g" output="out"/>
          </surface_unlit>
          <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
        </materialx>"#;
        let ir = crate::to_ir(text, None, None, &IrOptions::default(), &Resolver::default()).unwrap();
        let (patterns, written_text) = written(&ir.dag, false);
        assert!(written_text.contains(r#"<texcoord name="uv" type="vector2">"#), "{written_text}");
        assert!(written_text.contains(r#"<input name="index" type="integer" value="0" />"#), "{written_text}");
        assert!(written_text.contains(r#"<image name="img" type="color3">"#), "{written_text}");
        assert!(
            written_text.contains(r#"<input name="file" type="filename" value="a.png" colorspace="srgb_texture" />"#),
            "{written_text}"
        );
        assert!(
            written_text.contains(r#"<input name="uaddressmode" type="string" value="clamp" />"#),
            "{written_text}"
        );
        assert!(
            !written_text.contains("srgb_texture_to_lin_rec709"),
            "the conversion is folded into the image:\n{written_text}"
        );
        assert!(patterns.warnings.is_empty(), "{:?}", patterns.warnings);
    }

    #[test]
    fn a_kept_compound_is_one_node_and_its_interior_is_skipped() {
        let text = r#"<materialx version="1.39">
          <nodegraph name="g">
            <overlay name="o" type="color3">
              <input name="fg" type="color3" value="0.2, 0.4, 0.6"/>
              <input name="bg" type="color3" value="0.5, 0.5, 0.5"/>
            </overlay>
            <output name="out" type="color3" nodename="o"/>
          </nodegraph>
          <surface_unlit name="s" type="surfaceshader">
            <input name="emission_color" type="color3" nodegraph="g" output="out"/>
          </surface_unlit>
          <surfacematerial name="m" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s"/></surfacematerial>
        </materialx>"#;
        let ir = crate::to_ir(text, None, None, &IrOptions::default(), &Resolver::default()).unwrap();
        let (_, kept) = written(&ir.dag, false);
        assert!(kept.contains(r#"<overlay name="o" type="color3" nodedef="ND_overlay_color3">"#), "{kept}");
        // The graph's channel splits lower to `Op::Extract`, written as `extract`.
        assert!(!kept.contains("<extract"), "{kept}");
        // The mix input kept its nodedef default and is left out.
        assert!(!kept.contains(r#"name="mix""#), "{kept}");
        let (_, inlined) = written(&ir.dag, true);
        assert!(!inlined.contains("<overlay"), "{inlined}");
        assert!(inlined.contains("<extract"), "{inlined}");
    }
}
