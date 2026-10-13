//! Closure pruning: after constant folding, a lobe whose `weight` is the constant 0 (or whose constant black
//! `color` scales its whole response), a `multiply` by 0 and the
//! side of a `mix` its constant weight leaves out scatter nothing, so they leave the tree, and the fields only
//! they read leave the surface. Uniform weights stay: the tree's shape must not depend on uniform values.
//! Kept nodes are renumbered and their fields renamed to match (`c<id>_<input>`).

use std::collections::BTreeMap;

use crate::{ClosureGraph, ClosureId, ClosureNode, Dag, Node, NodeId, NodeKind, Op, SurfaceField, Type, Value};

/// The constant value of field `name`, when its node is a constant.
fn constant<'a>(dag: &'a Dag, name: &str) -> Option<&'a Value> {
    let field = dag.surface.fields.iter().find(|f| f.name == name)?;
    match &dag.node(field.node).kind {
        NodeKind::Const(value) => Some(value),
        _ => None,
    }
}

fn is_all(value: Option<&Value>, x: f32) -> bool {
    value.is_some_and(|v| !matches!(v, Value::Boolean(_) | Value::Integer(_)) && v.floats().iter().all(|f| *f == x))
}

/// Lobes whose whole response scales by their `color`: a constant black one scatters or emits nothing.
const COLOR_SCALED: [&str; 7] = [
    "oren_nayar_diffuse_bsdf",
    "burley_diffuse_bsdf",
    "translucent_bsdf",
    "sheen_bsdf",
    "subsurface_bsdf",
    "uniform_edf",
    "conical_edf",
];

/// The input part of a closure field name: `weight` for `c3_weight`.
fn suffix(field: &str) -> &str {
    field.split_once('_').map_or(field, |(_, rest)| rest)
}

struct Pruner<'a> {
    old: &'a Dag,
    out: ClosureGraph,
    /// Fields of kept nodes: (new name, old field node).
    fields: Vec<(String, Type, NodeId)>,
    /// Nodes added to the DAG for `1 - weight`.
    added: Vec<Node>,
}

impl Pruner<'_> {
    fn field_of(&self, name: &str) -> &SurfaceField {
        self.old.surface.fields.iter().find(|f| f.name == name).expect("closure field is a surface field")
    }

    /// Renames old field `name` for the node about to be added.
    fn keep_field(&mut self, name: &str) -> String {
        let field = self.field_of(name).clone();
        let new = format!("c{}_{}", self.out.nodes.len(), suffix(name));
        self.fields.push((new.clone(), field.ty, field.node));
        new
    }

    fn new_field(&mut self, input: &str, ty: Type, node: NodeId) -> String {
        let new = format!("c{}_{input}", self.out.nodes.len());
        self.fields.push((new.clone(), ty, node));
        new
    }

    fn add_node(&mut self, kind: NodeKind, ty: Type) -> NodeId {
        let id = NodeId((self.old.nodes.len() + self.added.len()) as u32);
        self.added.push(Node { kind, ty, origin: String::new() });
        id
    }

    fn prune(&mut self, map: &[Option<ClosureId>], node: &ClosureNode) -> Option<ClosureId> {
        let get = |c: &ClosureId| map[*c as usize];
        match node {
            ClosureNode::Lobe { kind, ty, inputs, settings, closures } => {
                if inputs.get("weight").is_some_and(|w| is_all(constant(self.old, w), 0.0)) {
                    return None;
                }
                let black = inputs.get("color").is_some_and(|c| is_all(constant(self.old, c), 0.0));
                if black && COLOR_SCALED.contains(&kind.as_str()) {
                    return None;
                }
                let mut kept = BTreeMap::new();
                for (name, child) in closures {
                    kept.insert(name.clone(), get(child)?);
                }
                let inputs = inputs.iter().map(|(k, v)| (k.clone(), self.keep_field(v))).collect();
                Some(self.out.add(ClosureNode::Lobe {
                    kind: kind.clone(),
                    ty: *ty,
                    inputs,
                    settings: settings.clone(),
                    closures: kept,
                }))
            }
            ClosureNode::Layer { top, base } => match (get(top), get(base)) {
                (Some(top), Some(base)) => Some(self.out.add(ClosureNode::Layer { top, base })),
                (one, None) | (None, one) => one,
            },
            ClosureNode::Add { in1, in2 } => match (get(in1), get(in2)) {
                (Some(in1), Some(in2)) => Some(self.out.add(ClosureNode::Add { in1, in2 })),
                (one, None) | (None, one) => one,
            },
            ClosureNode::Multiply { in1, in2 } => {
                let in1 = get(in1)?;
                let weight = constant(self.old, in2);
                if is_all(weight, 0.0) {
                    return None;
                }
                if is_all(weight, 1.0) {
                    return Some(in1);
                }
                let in2 = self.keep_field(in2);
                Some(self.out.add(ClosureNode::Multiply { in1, in2 }))
            }
            ClosureNode::Mix { fg, bg, mix } => {
                let weight = constant(self.old, mix);
                if is_all(weight, 0.0) {
                    return get(bg);
                }
                if is_all(weight, 1.0) {
                    return get(fg);
                }
                match (get(fg), get(bg)) {
                    (Some(fg), Some(bg)) => {
                        let mix = self.keep_field(mix);
                        Some(self.out.add(ClosureNode::Mix { fg, bg, mix }))
                    }
                    (Some(fg), None) => {
                        let in2 = self.keep_field(mix);
                        Some(self.out.add(ClosureNode::Multiply { in1: fg, in2 }))
                    }
                    (None, Some(bg)) => {
                        let w = self.field_of(mix).node;
                        let one = self.add_node(NodeKind::Const(Value::Float(1.0)), Type::Float);
                        let rest = self.add_node(NodeKind::Op { op: Op::Subtract, args: vec![one, w] }, Type::Float);
                        let in2 = self.new_field("in2", Type::Float, rest);
                        Some(self.out.add(ClosureNode::Multiply { in1: bg, in2 }))
                    }
                    (None, None) => None,
                }
            }
        }
    }
}

/// Prunes `dag`'s closure tree; a DAG without one is returned as is.
pub fn run(dag: &Dag) -> Dag {
    let Some(closure) = &dag.surface.closure else {
        return dag.clone();
    };
    let mut pruner = Pruner { old: dag, out: ClosureGraph::default(), fields: Vec::new(), added: Vec::new() };
    let mut map: Vec<Option<ClosureId>> = Vec::with_capacity(closure.nodes.len());
    for node in &closure.nodes {
        let kept = pruner.prune(&map, node);
        map.push(kept);
    }
    pruner.out.bsdf = closure.bsdf.and_then(|c| map[c as usize]);
    pruner.out.edf = closure.edf.and_then(|c| map[c as usize]);
    pruner.out.opacity = closure.opacity.clone();

    let Pruner { out, fields, added, .. } = pruner;
    let (out, fields) = compact(out, fields);
    let mut new = dag.clone();
    for node in added {
        new.add(node);
    }
    let referenced: Vec<String> = closure.nodes.iter().flat_map(|n| n.fields()).map(str::to_string).collect();
    let mut surface_fields: Vec<SurfaceField> =
        dag.surface.fields.iter().filter(|f| !referenced.contains(&f.name)).cloned().collect();
    surface_fields.extend(fields.into_iter().map(|(name, ty, node)| SurfaceField { name, ty, node }));
    new.surface.fields = surface_fields;
    new.surface.closure = Some(out);
    new
}

/// Drops the nodes the roots no longer reach (built before a parent decided against them), renumbers the
/// rest and renames their fields.
fn compact(graph: ClosureGraph, fields: Vec<(String, Type, NodeId)>) -> (ClosureGraph, Vec<(String, Type, NodeId)>) {
    let mut reached = vec![false; graph.nodes.len()];
    for root in graph.bsdf.iter().chain(&graph.edf) {
        reached[*root as usize] = true;
    }
    for index in (0..graph.nodes.len()).rev() {
        if reached[index] {
            for child in graph.nodes[index].children() {
                reached[child as usize] = true;
            }
        }
    }
    let mut map: Vec<Option<ClosureId>> = vec![None; graph.nodes.len()];
    let mut renamed: BTreeMap<String, String> = BTreeMap::new();
    let mut out = ClosureGraph { opacity: graph.opacity.clone(), ..ClosureGraph::default() };
    for (index, node) in graph.nodes.iter().enumerate() {
        if !reached[index] {
            continue;
        }
        let new_id = out.nodes.len() as ClosureId;
        let mut rename = |name: &String| {
            let new = format!("c{new_id}_{}", suffix(name));
            renamed.insert(name.clone(), new.clone());
            new
        };
        let child = |c: &ClosureId| map[*c as usize].expect("a reached node's children are reached");
        let node = match node {
            ClosureNode::Lobe { kind, ty, inputs, settings, closures } => ClosureNode::Lobe {
                kind: kind.clone(),
                ty: *ty,
                inputs: inputs.iter().map(|(k, v)| (k.clone(), rename(v))).collect(),
                settings: settings.clone(),
                closures: closures.iter().map(|(k, c)| (k.clone(), child(c))).collect(),
            },
            ClosureNode::Layer { top, base } => ClosureNode::Layer { top: child(top), base: child(base) },
            ClosureNode::Add { in1, in2 } => ClosureNode::Add { in1: child(in1), in2: child(in2) },
            ClosureNode::Mix { fg, bg, mix } => ClosureNode::Mix { fg: child(fg), bg: child(bg), mix: rename(mix) },
            ClosureNode::Multiply { in1, in2 } => ClosureNode::Multiply { in1: child(in1), in2: rename(in2) },
        };
        map[index] = Some(out.add(node));
    }
    out.bsdf = graph.bsdf.and_then(|c| map[c as usize]);
    out.edf = graph.edf.and_then(|c| map[c as usize]);
    let fields =
        fields.into_iter().filter_map(|(name, ty, node)| renamed.get(&name).map(|n| (n.clone(), ty, node))).collect();
    (out, fields)
}

#[cfg(test)]
mod tests {
    use super::run;
    use crate::*;
    use std::collections::BTreeMap;

    fn constant(dag: &mut Dag, name: &str, value: Value) {
        let ty = value.ty();
        let node = dag.add(Node { kind: NodeKind::Const(value), ty, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: name.into(), ty, node });
    }

    fn uniform(dag: &mut Dag, name: &str) {
        let node = dag.add(Node {
            kind: NodeKind::Uniform { name: name.into(), value: Value::Float(0.0) },
            ty: Type::Float,
            origin: String::new(),
        });
        dag.surface.fields.push(SurfaceField { name: name.into(), ty: Type::Float, node });
    }

    fn lobe(kind: &str, weight: &str) -> ClosureNode {
        ClosureNode::Lobe {
            kind: kind.into(),
            ty: ClosureType::Bsdf,
            inputs: BTreeMap::from([("weight".to_string(), weight.to_string())]),
            settings: BTreeMap::new(),
            closures: BTreeMap::new(),
        }
    }

    /// `layer(coat, mix(metal, diffuse, metalness))` with the given coat weight and metalness.
    fn coated(coat: Value, metalness: Option<Value>) -> Dag {
        let mut dag = Dag::new("surface");
        constant(&mut dag, "c0_weight", coat);
        constant(&mut dag, "c1_weight", Value::Float(1.0));
        constant(&mut dag, "c2_weight", Value::Float(1.0));
        match metalness {
            Some(v) => constant(&mut dag, "c3_mix", v),
            None => uniform(&mut dag, "c3_mix"),
        }
        constant(&mut dag, "opacity", Value::Float(1.0));
        let mut g = ClosureGraph::default();
        let coat = g.add(lobe("dielectric_bsdf", "c0_weight"));
        let metal = g.add(lobe("conductor_bsdf", "c1_weight"));
        let diffuse = g.add(lobe("oren_nayar_diffuse_bsdf", "c2_weight"));
        let base = g.add(ClosureNode::Mix { fg: metal, bg: diffuse, mix: "c3_mix".into() });
        g.bsdf = Some(g.add(ClosureNode::Layer { top: coat, base }));
        g.opacity = Some("opacity".into());
        dag.surface.closure = Some(g);
        dag
    }

    fn names(dag: &Dag) -> Vec<&str> {
        dag.surface.fields.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn a_zero_weight_lobe_and_a_decided_mix_leave_the_tree() {
        let out = run(&coated(Value::Float(0.0), Some(Value::Float(0.0))));
        assert_eq!(
            out.surface.closure.as_ref().unwrap().dump(),
            "c0 = oren_nayar_diffuse_bsdf(weight=c0_weight)\nbsdf = c0\nedf = none\nopacity = opacity\n"
        );
        assert_eq!(names(&out), ["opacity", "c0_weight"]);
    }

    #[test]
    fn a_black_emitter_leaves_the_tree() {
        let mut dag = Dag::new("surface");
        constant(&mut dag, "c0_color", Value::Color3([0.0; 3]));
        let mut g = ClosureGraph::default();
        g.edf = Some(g.add(ClosureNode::Lobe {
            kind: "uniform_edf".into(),
            ty: ClosureType::Edf,
            inputs: BTreeMap::from([("color".to_string(), "c0_color".to_string())]),
            settings: BTreeMap::new(),
            closures: BTreeMap::new(),
        }));
        dag.surface.closure = Some(g);
        let out = run(&dag);
        assert_eq!(out.surface.closure.as_ref().unwrap().edf, None);
        assert!(out.surface.fields.is_empty());
    }

    #[test]
    fn uniform_weights_keep_the_shape() {
        let out = run(&coated(Value::Float(0.5), None));
        let dump = out.surface.closure.as_ref().unwrap().dump();
        assert!(dump.contains("c3 = mix(fg=c1, bg=c2, mix=c3_mix)\nc4 = layer(top=c0, base=c3)\nbsdf = c4"), "{dump}");
        assert_eq!(names(&out).len(), 5);
    }

    #[test]
    fn a_mix_missing_its_fg_scales_the_bg_by_one_minus_the_weight() {
        let mut dag = coated(Value::Float(0.0), None);
        let g = dag.surface.closure.as_mut().unwrap();
        g.nodes[1] = lobe("conductor_bsdf", "c0_weight");
        let out = run(&dag);
        let dump = out.surface.closure.as_ref().unwrap().dump();
        assert!(dump.contains("c1 = multiply(c0, c1_in2)\nbsdf = c1"), "{dump}");
        let field = out.surface.fields.iter().find(|f| f.name == "c1_in2").unwrap();
        assert!(matches!(out.node(field.node).kind, NodeKind::Op { op: Op::Subtract, .. }));
    }
}
