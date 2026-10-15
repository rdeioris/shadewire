//! How the shader reads the nodegraph: values on its inputs, or nodegraph outputs made on demand.

use shadewire::{Dag, NodeId, NodeKind};

use super::elements::{Source, dot_element, output_element};
use super::patterns::Patterns;
use super::values::value_text;

/// How a shader-level input (a surface field, a lobe input) reaches a dag node: `value` for an inlinable constant
/// or a uniform whose name is the input's, else a nodegraph output made on demand.
#[derive(Clone, Debug, PartialEq)]
pub enum Reach {
    Value(String),
    Output(String),
}

impl Patterns<'_> {
    /// How a shader-level input named `input_name` reads `node`. A uniform of that name read there only (its
    /// nodegraph input not written for another reader) is the input's value, when `value_allowed` (an input with
    /// a `defaultgeomprop` ignores a value on read). Anything else is a nodegraph output, made once per node and
    /// named after the first input that asks: a const through a `constant` node (a value on a shader input would
    /// read back as a uniform), another uniform through a `dot` reading its nodegraph input.
    ///
    /// Panics when `node` is inside a kept compound without being one of its outputs: the plan counts every
    /// shader read, so such a node is never asked for.
    pub fn reach(&mut self, node: NodeId, input_name: &str, value_allowed: bool, dag: &Dag) -> Reach {
        let node = self.aliases.get(&node).copied().unwrap_or(node);
        let n = dag.node(node);
        if let NodeKind::Uniform { value, .. } = &n.kind
            && value_allowed
            && self.uniform_names.get(&node) == Some(&input_name)
            && self.readers.get(&node) == Some(&1)
            && self.pending_uniforms.contains_key(&node)
        {
            return Reach::Value(value_text(value));
        }
        if let Some(output) = self.reached.get(&node) {
            return Reach::Output(output.clone());
        }
        self.write_leaf(dag, node);
        let written =
            self.node_names.get(&node).cloned().unwrap_or_else(|| panic!("reach: node {} not written", node.0));
        let source = if self.uniform_names.contains_key(&node) {
            self.write_uniform(node);
            let dot = self.names.unique(&format!("{written}_dot"));
            self.nodes.push(dot_element(&dot, n.ty, &written));
            Source::Node { name: dot, output: None }
        } else {
            Source::Node { name: written, output: self.node_outputs.get(&node).cloned() }
        };
        let name = match self.names.reserve(input_name) {
            true => input_name.to_string(),
            false => self.names.unique(&format!("{input_name}_out")),
        };
        self.outputs.push(output_element(&name, n.ty, &source));
        self.reached.insert(node, name.clone());
        Reach::Output(name)
    }
}

#[cfg(test)]
mod tests {
    use super::super::WriteOptions;
    use super::super::names::Names;
    use super::super::patterns::write_patterns;
    use super::super::plan::plan;
    use super::super::xml::to_text;
    use super::*;
    use shadewire::*;
    use shadewire_nodes::Index;
    use shadewire_nodes::document::empty_document;
    use shadewire_nodes::library::standard;

    fn node(kind: NodeKind, ty: Type, origin: &str) -> Node {
        Node { kind, ty, origin: origin.to_string() }
    }

    /// Reaches the fields named in `fields` under their own names; returns the reaches and the nodegraph's text.
    fn reached(dag: &Dag, fields: &[&str]) -> (Vec<Reach>, String) {
        let index = Index::new(empty_document(), standard());
        let plan = plan(dag, &index, false);
        let mut names = Names::new();
        let mut patterns = write_patterns(dag, &plan, &index, &WriteOptions::default(), &mut names).unwrap();
        let wanted = dag.surface.fields.iter().filter(|f| fields.contains(&f.name.as_str()));
        let reaches = wanted.map(|f| patterns.reach(f.node, &f.name, true, dag)).collect();
        let mut graph = super::super::xml::Element::new("nodegraph");
        for e in patterns.inputs.iter().chain(&patterns.nodes).chain(&patterns.outputs) {
            graph.push(e.clone());
        }
        (reaches, to_text(&graph))
    }

    #[test]
    fn a_uniform_named_after_its_only_reader_is_a_value_and_no_nodegraph_input() {
        let mut dag = Dag::new("surface_unlit");
        let u = dag.add(node(NodeKind::Uniform { name: "emission".into(), value: Value::Float(0.5) }, Type::Float, ""));
        dag.surface.fields.push(SurfaceField { name: "emission".into(), ty: Type::Float, node: u });
        let (reaches, text) = reached(&dag, &["emission"]);
        assert_eq!(reaches, [Reach::Value("0.5".into())]);
        assert!(!text.contains("emission"), "{text}");
    }

    #[test]
    fn a_uniform_already_read_by_an_op_is_never_also_a_value() {
        let mut dag = Dag::new("surface_unlit");
        let u = dag.add(node(NodeKind::Uniform { name: "emission".into(), value: Value::Float(0.5) }, Type::Float, ""));
        let s = dag.add(node(NodeKind::Op { op: Op::Sin, args: vec![u] }, Type::Float, "g/s"));
        dag.surface.fields.push(SurfaceField { name: "emission".into(), ty: Type::Float, node: s });
        let index = Index::new(empty_document(), standard());
        let plan = plan(&dag, &index, false);
        let mut patterns = write_patterns(&dag, &plan, &index, &WriteOptions::default(), &mut Names::new()).unwrap();
        assert_eq!(patterns.reach(u, "emission", true, &dag), Reach::Output("emission_out".into()));
        assert_eq!(patterns.inputs.len(), 1, "one nodegraph input, read by the op and the output");
    }

    #[test]
    fn a_const_or_another_uniform_is_a_nodegraph_output_made_once() {
        let mut dag = Dag::new("surface_unlit");
        let c = dag.add(node(NodeKind::Const(Value::Float(2.0)), Type::Float, "s.emission"));
        let u = dag.add(node(NodeKind::Uniform { name: "gain".into(), value: Value::Float(0.5) }, Type::Float, ""));
        for (name, id) in [("emission", c), ("transmission", u), ("opacity", u)] {
            dag.surface.fields.push(SurfaceField { name: name.into(), ty: Type::Float, node: id });
        }
        let (reaches, text) = reached(&dag, &["emission", "transmission", "opacity"]);
        let outputs = ["emission", "transmission", "transmission"].map(|o| Reach::Output(o.into()));
        assert_eq!(reaches, outputs);
        assert!(text.contains(r#"<constant name="s_emission" type="float">"#), "{text}");
        assert!(text.contains(r#"<output name="emission" type="float" nodename="s_emission" />"#), "{text}");
        assert!(text.contains(r#"<input name="gain" type="float" value="0.5" />"#), "{text}");
        assert!(text.contains(r#"<input name="in" type="float" interfacename="gain" />"#), "{text}");
        assert!(text.contains(r#"<output name="transmission" type="float" nodename="gain_dot" />"#), "{text}");
    }

    #[test]
    fn a_context_read_by_the_shader_only_is_written_when_reached() {
        let mut dag = Dag::new("surface_unlit");
        let normal = dag.add(node(NodeKind::Context(ContextRead::Normal(Space::World)), Type::Vector3, "s.normal"));
        let read = ContextRead::TexCoord { index: 1, ty: Type::Vector2 };
        let uv = dag.add(node(NodeKind::Context(read), Type::Vector2, ""));
        dag.surface.fields.push(SurfaceField { name: "normal".into(), ty: Type::Vector3, node: normal });
        dag.surface.fields.push(SurfaceField { name: "uv".into(), ty: Type::Vector2, node: uv });
        let (reaches, text) = reached(&dag, &["uv"]);
        assert_eq!(reaches, [Reach::Output("uv".into())]);
        assert!(!text.contains("<normal"), "{text}");
        assert!(text.contains(r#"<texcoord name="texcoord_1" type="vector2">"#), "{text}");
        assert!(text.contains(r#"<output name="uv" type="vector2" nodename="texcoord_1" />"#), "{text}");
    }
}
