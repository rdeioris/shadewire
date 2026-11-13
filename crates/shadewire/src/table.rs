use crate::{Type, Value};

/// A constant array the shader reads at an index known only at run time (`NodeKind::Lookup`): an MDL `const
/// int[256]` permutation table indexed by a hash of the position, say. Every value is of the element type `ty` (a
/// scalar or a vector); a back end emits the table once, however many lookups read it.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub ty: Type,
    pub values: Vec<Value>,
}

impl Table {
    /// The value at `index` (an integer, or a float truncated toward zero), clamped to the table as every back
    /// end clamps it; the element type's zero for an empty table.
    pub fn get(&self, index: &Value) -> Value {
        if self.values.is_empty() {
            return Value::zero(self.ty);
        }
        let i = match index {
            Value::Integer(i) => i64::from(*i),
            other => other.floats().first().map_or(0, |x| *x as i64),
        };
        let last = self.values.len() as i64 - 1;
        self.values[i.clamp(0, last) as usize].clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::interpret::ContextSource;
    use crate::*;

    struct Zero;
    impl ContextSource for Zero {
        fn read(&mut self, read: &ContextRead) -> Value {
            Value::zero(read.ty())
        }
        fn transform(&mut self, _: TransformKind, _: Space, _: Space, v: [f32; 3]) -> [f32; 3] {
            v
        }
    }

    fn lookup_dag(index: NodeKind) -> Dag {
        let mut dag = Dag::new("m");
        let table = dag.add_table(Table { ty: Type::Float, values: vec![Value::Float(1.0), Value::Float(2.0)] });
        assert_eq!(dag.add_table(Table { ty: Type::Float, values: vec![Value::Float(1.0), Value::Float(2.0)] }), table);
        let i = dag.add(Node { kind: index, ty: Type::Integer, origin: String::new() });
        let read = dag.add(Node { kind: NodeKind::Lookup { table, index: i }, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: read });
        dag
    }

    #[test]
    fn an_index_is_clamped_to_the_table() {
        let t = Table { ty: Type::Integer, values: vec![Value::Integer(7), Value::Integer(8)] };
        assert_eq!(t.get(&Value::Integer(-3)), Value::Integer(7));
        assert_eq!(t.get(&Value::Integer(1)), Value::Integer(8));
        assert_eq!(t.get(&Value::Integer(99)), Value::Integer(8));
        assert_eq!(t.get(&Value::Float(1.9)), Value::Integer(8));
        assert_eq!(Table { ty: Type::Float, values: Vec::new() }.get(&Value::Integer(0)), Value::Float(0.0));
    }

    #[test]
    fn a_lookup_at_a_constant_index_folds() {
        let dag = optimize::optimize(&lookup_dag(NodeKind::Const(Value::Integer(1))));
        assert_eq!(dag.node(dag.surface.fields[0].node).kind, NodeKind::Const(Value::Float(2.0)));
    }

    #[test]
    fn a_lookup_at_a_uniform_index_keeps_its_table_through_the_optimizer() {
        let dag = lookup_dag(NodeKind::Uniform { name: "i".into(), value: Value::Integer(0) });
        let optimized = optimize::optimize(&dag);
        assert_eq!(optimized.tables, dag.tables);
        assert!(optimized.dump().contains("lookup L0[%0]"), "{}", optimized.dump());
        assert_eq!(
            interpret::eval_surface(&optimized, &mut Zero, &interpret::NoTextures),
            [("base_weight".to_string(), Value::Float(1.0))]
        );
        let json = serde_json::to_value(export::export(&optimized)).unwrap();
        assert_eq!(json["tables"], serde_json::json!([{"type": "float", "values": [[1.0], [2.0]]}]));
        assert_eq!(json["nodes"][1], serde_json::json!({"kind": "lookup", "type": "float", "table": 0, "args": [0]}));
    }
}
