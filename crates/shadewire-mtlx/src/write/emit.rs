//! Writing one sample, op or kept compound into the nodegraph, its inputs read from the nodes already written.

use shadewire::{Dag, Error, Location, NodeId, NodeKind, Value};
use shadewire_nodes::Index;
use shadewire_nodes::document::{NodeDef, PortType};

use super::catalog::{arg_names, category_for, is_shadewire_category, nodedef_for};
use super::elements::{Source, compound_element, image_element, op_element};
use super::patterns::Patterns;
use super::plan::{Plan, converted_sample};
use super::values::value_text;

/// The value an unconnected nodedef input reads: its declared value.
fn nodedef_default(nd: &NodeDef, input: &str) -> Option<Value> {
    let port = nd.inputs.iter().find(|p| p.name == input)?;
    match (&port.port_type, port.value.as_deref()) {
        (PortType::Value(ty), Some(text)) if !text.trim().is_empty() => Value::parse(*ty, text).ok(),
        _ => None,
    }
}

/// Whether a sample's image carries its texture's `colorspace` (which converts on read): when its conversion is
/// folded into it, or when no conversion read it (none applied: a non-color sample, `none`/`raw`, the working
/// colorspace). A conversion written as its own nodes already converts.
fn keeps_colorspace(dag: &Dag, plan: &Plan, sample: NodeId) -> bool {
    plan.folded_conversions.values().any(|folded| *folded == sample)
        || !dag.compounds.iter().any(|c| converted_sample(dag, c) == Some(sample))
}

impl Patterns<'_> {
    /// How a written node reads `id`: its value when the plan inlines it, a nodegraph input, or a node (a later
    /// copy of a kept compound's output read from the compound).
    fn source(&self, dag: &Dag, plan: &Plan, id: NodeId) -> Result<Source, Error> {
        let id = self.aliases.get(&id).copied().unwrap_or(id);
        if let (true, NodeKind::Const(value)) = (plan.inline_const.contains(&id), &dag.node(id).kind) {
            return Ok(Source::Value(value_text(value)));
        }
        let name = self.node_names.get(&id).ok_or_else(|| Error::BadValue {
            at: Location { file: None, line: 0, path: dag.node(id).origin.clone() },
            message: format!("node {} is read but not written", id.0),
        })?;
        if self.uniform_names.contains_key(&id) {
            return Ok(Source::Interface(name.clone()));
        }
        Ok(Source::Node { name: name.clone(), output: self.node_outputs.get(&id).cloned() })
    }

    /// An `image` node, or `shadewire_image_grad` for an explicit-gradient lookup; it also stands for the
    /// colorspace conversions folded into it.
    pub(super) fn write_sample(&mut self, dag: &Dag, plan: &Plan, id: NodeId) -> Result<(), Error> {
        let node = dag.node(id);
        let NodeKind::Sample { texture, uv, default, grad } = &node.kind else { return Ok(()) };
        let uv = self.source(dag, plan, *uv)?;
        let grad = match grad {
            Some([dx, dy]) => Some([self.source(dag, plan, *dx)?, self.source(dag, plan, *dy)?]),
            None => None,
        };
        let category = if grad.is_some() { "shadewire_image_grad" } else { "image" };
        let name = self.node_name(node, category, id.index());
        let mut texture = dag.textures[*texture].clone();
        if !keeps_colorspace(dag, plan, id) {
            texture.colorspace = None;
        }
        let mut element = image_element(category, &name, node.ty, &texture, default, &uv, grad);
        if is_shadewire_category(category) {
            element = element.attr("nodedef", format!("ND_{category}_{}", node.ty.mtlx_name()));
            self.warn_shadewire(category);
        }
        self.nodes.push(element);
        for (compound, sample) in &plan.folded_conversions {
            if *sample == id {
                for (_, output) in &dag.compounds[*compound].outputs {
                    self.node_names.insert(*output, name.clone());
                }
            }
        }
        self.node_names.insert(id, name);
        Ok(())
    }

    /// An op as its library node, its arguments by the nodedef's input names and its parameters as values.
    pub(super) fn write_op(&mut self, dag: &Dag, plan: &Plan, index: &Index, id: NodeId) -> Result<(), Error> {
        let node = dag.node(id);
        let NodeKind::Op { op, args } = &node.kind else { return Ok(()) };
        let category = category_for(op, args.len());
        let arg_types: Vec<_> = args.iter().map(|a| dag.node(*a).ty).collect();
        let nd = nodedef_for(index, &category.name, node.ty, &arg_types).ok_or_else(|| Error::BadValue {
            at: Location { file: None, line: 0, path: node.origin.clone() },
            message: format!("no library node writes {op:?} as {}", node.ty.mtlx_name()),
        })?;
        let mut inputs = Vec::new();
        for (input, arg) in arg_names(op, nd).into_iter().zip(args) {
            inputs.push((input, self.source(dag, plan, *arg)?));
        }
        let name = self.node_name(node, &category.name, id.index());
        self.nodes.push(op_element(&name, &category, nd, node.ty, &inputs));
        if is_shadewire_category(&category.name) {
            self.warn_shadewire(&category.name);
        }
        self.node_names.insert(id, name);
        Ok(())
    }

    /// A kept compound as one node of its nodedef, named after its origin; inputs still at the nodedef's
    /// default are left out. Its outputs are read from it (by `output=` when it has several).
    pub(super) fn write_compound(
        &mut self,
        dag: &Dag,
        index: &Index,
        plan: &Plan,
        position: usize,
    ) -> Result<(), Error> {
        let compound = &dag.compounds[position];
        let Some(nd) = index.nodedef(&compound.nodedef) else { return Ok(()) };
        let mut inputs = Vec::new();
        for (input, id) in &compound.inputs {
            if let NodeKind::Const(value) = &dag.node(*id).kind
                && nodedef_default(nd, input).as_ref() == Some(value)
            {
                continue;
            }
            inputs.push((input.as_str(), self.source(dag, plan, *id)?));
        }
        let name = self.names.unique(&compound.origin);
        self.nodes.push(compound_element(&name, nd, &inputs));
        for (output, id) in &compound.outputs {
            if nd.outputs.len() > 1 {
                self.node_outputs.insert(*id, output.clone());
            }
            self.node_names.insert(*id, name.clone());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::WriteOptions;
    use super::super::names::Names;
    use super::super::patterns::write_patterns;
    use super::super::plan::plan;
    use super::super::xml::{Element, to_text};
    use crate::{IrOptions, Resolver};
    use shadewire_nodes::Index;
    use shadewire_nodes::document::empty_document;
    use shadewire_nodes::library::standard;

    /// The nodes of nodegraph `g`, read by a `surface_unlit` through `emission_color`, after `defs`.
    fn written_nodes(defs: &str, graph: &str) -> String {
        let text = format!(
            r#"<materialx version="1.39">{defs}
              <nodegraph name="g">{graph}</nodegraph>
              <surface_unlit name="u" type="surfaceshader">
                <input name="emission_color" type="color3" nodegraph="g" output="out"/>
              </surface_unlit>
              <surfacematerial name="m" type="material">
                <input name="surfaceshader" type="surfaceshader" nodename="u"/>
              </surfacematerial>
            </materialx>"#
        );
        let dag = crate::to_ir(&text, None, None, &IrOptions::default(), &Resolver::default()).unwrap().dag;
        let index = Index::new(empty_document(), standard());
        let plan = plan(&dag, &index, false);
        let patterns = write_patterns(&dag, &plan, &index, &WriteOptions::default(), &mut Names::new()).unwrap();
        let mut graph = Element::new("nodegraph");
        for e in patterns.nodes {
            graph.push(e);
        }
        to_text(&graph)
    }

    #[test]
    fn a_kept_compound_inlined_twice_is_one_node() {
        // Following both outputs of `t` inlines its graph twice; `Compound` records the overlay of the first copy.
        let defs = r#"<nodedef name="ND_two" node="two">
            <input name="in" type="color3" value="0.2, 0.2, 0.2"/>
            <output name="a" type="color3"/><output name="b" type="color3"/>
          </nodedef>
          <nodegraph name="NG_two" nodedef="ND_two">
            <overlay name="o" type="color3">
              <input name="fg" type="color3" interfacename="in"/><input name="bg" type="color3" value="0.5, 0.5, 0.5"/>
            </overlay>
            <output name="a" type="color3" nodename="o"/><output name="b" type="color3" nodename="o"/>
          </nodegraph>"#;
        let graph = r#"<two name="t" type="multioutput"><input name="in" type="color3" value="0.3, 0.3, 0.3"/></two>
            <add name="s" type="color3">
              <input name="in1" type="color3" nodename="t" output="a"/>
              <input name="in2" type="color3" nodename="t" output="b"/>
            </add>
            <output name="out" type="color3" nodename="s"/>"#;
        let written = written_nodes(defs, graph);
        assert_eq!(written.matches("<overlay ").count(), 1, "{written}");
        assert!(written.contains(r#"<input name="in1" type="color3" nodename="o" />"#), "{written}");
        assert!(written.contains(r#"<input name="in2" type="color3" nodename="o" />"#), "{written}");
    }

    #[test]
    fn an_image_carries_its_colorspace_unless_a_conversion_it_did_not_fold_reads_it() {
        use shadewire::*;
        let mut dag = Dag::new("surface_unlit");
        let texture = |file: &str, colorspace: &str| TextureDesc {
            file: file.into(),
            colorspace: Some(colorspace.into()),
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        };
        let (srgb, linear) =
            (dag.add_texture(texture("a.png", "srgb_texture")), dag.add_texture(texture("b.png", "lin_rec709")));
        let read = ContextRead::TexCoord { index: 0, ty: Type::Vector2 };
        let uv = dag.add(Node { kind: NodeKind::Context(read), ty: Type::Vector2, origin: "g/uv".into() });
        let sample = |texture, origin: &str| Node {
            kind: NodeKind::Sample { texture, uv, default: Value::Color3([0.0; 3]), grad: None },
            ty: Type::Color3,
            origin: origin.into(),
        };
        let a = dag.add(sample(srgb, "g/a"));
        let b = dag.add(sample(linear, "g/b"));
        // `a`'s conversion computes its output outside its own origin, so it is not folded into `a`'s image.
        let sum = dag.add(Node {
            kind: NodeKind::Op { op: Op::Add, args: vec![a, b] },
            ty: Type::Color3,
            origin: "g/s".into(),
        });
        dag.compounds.push(Compound {
            nodedef: "ND_srgb_texture_to_lin_rec709_color3".into(),
            inputs: vec![("in".into(), a)],
            outputs: vec![("out".into(), sum)],
            origin: "g/a/convert".into(),
        });
        dag.surface.fields.push(SurfaceField { name: "emission_color".into(), ty: Type::Color3, node: sum });
        let index = Index::new(empty_document(), standard());
        let plan = plan(&dag, &index, false);
        let patterns = write_patterns(&dag, &plan, &index, &WriteOptions::default(), &mut Names::new()).unwrap();
        let mut graph = Element::new("nodegraph");
        graph.children = patterns.nodes;
        let written = to_text(&graph);
        assert!(written.contains(r#"<input name="file" type="filename" value="a.png" />"#), "{written}");
        assert!(written.contains(r#"value="b.png" colorspace="lin_rec709" />"#), "{written}");
    }

    #[test]
    fn a_multi_output_compound_is_read_by_output_name() {
        let graph = r#"<artistic_ior name="a" type="multioutput">
              <input name="reflectivity" type="color3" value="0.9, 0.5, 0.3"/>
            </artistic_ior>
            <add name="s" type="color3">
              <input name="in1" type="color3" nodename="a" output="ior"/>
              <input name="in2" type="color3" nodename="a" output="extinction"/>
            </add>
            <output name="out" type="color3" nodename="s"/>"#;
        let written = written_nodes("", graph);
        let head = r#"<artistic_ior name="a" type="multioutput" nodedef="ND_artistic_ior">"#;
        assert!(written.contains(head), "{written}");
        assert!(written.contains(r#"<input name="reflectivity" type="color3" value="0.9, 0.5, 0.3" />"#), "{written}");
        assert!(!written.contains(r#"name="edge_color""#), "{written}");
        assert!(written.contains(r#"<input name="in1" type="color3" nodename="a" output="ior" />"#), "{written}");
        assert!(
            written.contains(r#"<input name="in2" type="color3" nodename="a" output="extinction" />"#),
            "{written}"
        );
        assert_eq!(written.matches("<artistic_ior ").count(), 1, "{written}");
    }
}
