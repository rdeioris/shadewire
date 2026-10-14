//! A library node instantiated as a subgraph: what it was, so a graph back end can write it as one node again.
//! Metadata: the interpreter, the structure hash and the shader back ends ignore compounds, and the optimizer
//! drops them (after folding, their interiors may be gone).

use crate::NodeId;

#[derive(Clone, Debug, PartialEq)]
pub struct Compound {
    /// The nodedef instantiated (`ND_overlay_color3`).
    pub nodedef: String,
    /// The nodedef's value inputs that were bound, each to the IR node bound to it, in nodedef order.
    pub inputs: Vec<(String, NodeId)>,
    /// The outputs followed so far: nodedef output name to the IR node computing it.
    pub outputs: Vec<(String, NodeId)>,
    /// The instance's path (`NG_marble/noise1`): the `origin` of the nodes inside starts with it.
    pub origin: String,
}

#[cfg(test)]
mod tests {
    use crate::optimize::optimize;
    use crate::*;

    fn dag_with_compound() -> Dag {
        let mut dag = Dag::new("surface_unlit");
        let a = dag.add(Node { kind: NodeKind::Const(Value::Float(1.0)), ty: Type::Float, origin: "x/a".into() });
        let b =
            dag.add(Node { kind: NodeKind::Op { op: Op::Sin, args: vec![a] }, ty: Type::Float, origin: "x/b".into() });
        dag.surface.fields.push(SurfaceField { name: "emission".into(), ty: Type::Float, node: b });
        dag.compounds.push(Compound {
            nodedef: "ND_x_float".into(),
            inputs: vec![("in".into(), a)],
            outputs: vec![("out".into(), b)],
            origin: "x".into(),
        });
        dag
    }

    #[test]
    fn compounds_do_not_change_the_structure_hash() {
        let with = dag_with_compound();
        let mut without = with.clone();
        without.compounds.clear();
        assert_eq!(structure_hash(&with), structure_hash(&without));
    }

    #[test]
    fn the_optimizer_drops_compounds() {
        assert!(optimize(&dag_with_compound()).compounds.is_empty());
    }

    #[test]
    fn the_export_lists_compounds() {
        let json = serde_json::to_string(&export::export(&dag_with_compound()).compounds).unwrap();
        assert_eq!(json, r#"[{"nodedef":"ND_x_float","inputs":[["in",0]],"outputs":[["out",1]],"origin":"x"}]"#);
    }
}
