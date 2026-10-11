use crate::{Dag, NodeKind, Value};

/// 64-bit FNV-1a, written out so the hash does not depend on the standard library.
struct Fnv(u64);

impl Fnv {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    fn new() -> Fnv {
        Fnv(Fnv::OFFSET)
    }

    fn byte(&mut self, b: u8) {
        self.0 ^= b as u64;
        self.0 = self.0.wrapping_mul(Fnv::PRIME);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.byte(*b);
        }
    }

    fn number(&mut self, n: u64) {
        self.bytes(&n.to_le_bytes());
    }

    /// Length-prefixed, so "ab" then "c" hashes differently from "a" then "bc".
    fn text(&mut self, s: &str) {
        self.number(s.len() as u64);
        self.bytes(s.as_bytes());
    }

    fn float(&mut self, x: f32) {
        self.bytes(&x.to_bits().to_le_bytes());
    }

    /// Booleans and integers are hashed natively, so large integers stay distinct.
    fn value(&mut self, value: &Value) {
        self.text(value.ty().mtlx_name());
        match value {
            Value::Boolean(b) => self.byte(*b as u8),
            Value::Integer(i) => self.bytes(&i.to_le_bytes()),
            _ => {
                for x in value.floats() {
                    self.float(x);
                }
            }
        }
    }
}

/// A hash of the DAG's shape. Uniform values are left out, so changing one does not change it.
pub fn structure_hash(dag: &Dag) -> String {
    let mut h = Fnv::new();
    h.text(&dag.surface.model);
    h.number(dag.nodes.len() as u64);
    for node in &dag.nodes {
        match &node.kind {
            NodeKind::Const(value) => {
                h.byte(0);
                h.value(value);
            }
            NodeKind::Uniform { name, .. } => {
                h.byte(1);
                h.text(name);
            }
            NodeKind::Context(read) => {
                h.byte(2);
                h.text(&read.reflection_name());
            }
            NodeKind::Sample { texture, uv, default, grad } => {
                h.byte(3);
                let desc = &dag.textures[*texture];
                h.text(&desc.file);
                h.text(desc.colorspace.as_deref().unwrap_or(""));
                h.text(&format!("{:?}{:?}{:?}", desc.address_u, desc.address_v, desc.filter));
                h.number(uv.0 as u64);
                h.value(default);
                for id in grad.iter().flatten() {
                    h.number(id.0 as u64);
                }
            }
            NodeKind::Op { op, args } => {
                h.byte(4);
                h.text(&format!("{op:?}"));
                h.number(args.len() as u64);
                for arg in args {
                    h.number(arg.0 as u64);
                }
            }
        }
        h.text(node.ty.mtlx_name());
    }
    h.number(dag.surface.fields.len() as u64);
    for field in &dag.surface.fields {
        h.text(&field.name);
        h.number(field.node.0 as u64);
    }
    format!("{:016x}", h.0)
}

#[cfg(test)]
mod tests {
    use crate::*;

    fn dag_with_uniform(value: f32) -> Dag {
        let mut dag = Dag::new("open_pbr_surface");
        let u = dag.add(Node {
            kind: NodeKind::Uniform { name: "base_weight".into(), value: Value::Float(value) },
            ty: Type::Float,
            origin: String::new(),
        });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: u });
        dag
    }

    #[test]
    fn uniform_values_do_not_change_the_hash() {
        assert_eq!(structure_hash(&dag_with_uniform(0.2)), structure_hash(&dag_with_uniform(0.9)));
    }

    #[test]
    fn large_integer_constants_hash_differently() {
        let int_dag = |n: i32| {
            let mut dag = Dag::new("m");
            let c =
                dag.add(Node { kind: NodeKind::Const(Value::Integer(n)), ty: Type::Integer, origin: String::new() });
            dag.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Integer, node: c });
            dag
        };
        assert_ne!(structure_hash(&int_dag(16777216)), structure_hash(&int_dag(16777217)));
    }

    #[test]
    fn constants_do_change_it() {
        let mut a = Dag::new("m");
        let mut b = Dag::new("m");
        let ca = a.add(Node { kind: NodeKind::Const(Value::Float(1.0)), ty: Type::Float, origin: String::new() });
        let cb = b.add(Node { kind: NodeKind::Const(Value::Float(2.0)), ty: Type::Float, origin: String::new() });
        a.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Float, node: ca });
        b.surface.fields.push(SurfaceField { name: "x".into(), ty: Type::Float, node: cb });
        assert_ne!(structure_hash(&a), structure_hash(&b));
    }
}
