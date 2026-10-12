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

/// A hash of everything in the DAG that ends up in generated code: its shape, constants, textures and
/// the geomprop defaults (hosts return them as literals). Uniform values are left out, so changing one
/// does not change it. The generated WGSL also depends on the bind group, the first binding, `flip_v`
/// and `reserve_uniform_binding`, which are not part of the DAG: a shader cache must key on
/// `(structure_hash, group, first_binding, flip_v, reserve_uniform_binding)`.
///
/// A DAG without geomprop defaults hashes exactly as before they were hashed; the defaults, when there
/// are any, are a tagged section after the surface fields, in name order (a `BTreeMap`).
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
                h.number(*texture as u64);
                let desc = &dag.textures[*texture];
                h.text(&desc.file);
                h.text(desc.colorspace.as_deref().unwrap_or(""));
                h.text(&format!("{:?}{:?}{:?}", desc.address_u, desc.address_v, desc.filter));
                h.number(uv.0 as u64);
                h.value(default);
                match grad {
                    None => h.byte(0),
                    Some([dx, dy]) => {
                        h.byte(1);
                        h.number(dx.0 as u64);
                        h.number(dy.0 as u64);
                    }
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
    if !dag.geomprop_defaults.is_empty() {
        h.byte(5);
        h.number(dag.geomprop_defaults.len() as u64);
        for (name, value) in &dag.geomprop_defaults {
            h.text(name);
            h.value(value);
        }
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

    /// A geomprop default is written into the host's code (the reference host and Bevy return it), so it
    /// is structure: its name, type and value all change the hash.
    #[test]
    fn geomprop_defaults_change_the_hash() {
        let dag = |default: Option<(&str, Value)>| {
            let mut dag = Dag::new("m");
            let read = ContextRead::GeomProp { name: "tint".into(), ty: Type::Color3 };
            let g = dag.add(Node { kind: NodeKind::Context(read), ty: Type::Color3, origin: String::new() });
            dag.surface.fields.push(SurfaceField { name: "c".into(), ty: Type::Color3, node: g });
            if let Some((name, value)) = default {
                dag.geomprop_defaults.insert(name.into(), value);
            }
            dag
        };
        let hashes = [
            structure_hash(&dag(None)),
            structure_hash(&dag(Some(("tint", Value::Color3([1.0, 1.0, 1.0]))))),
            structure_hash(&dag(Some(("tint", Value::Color3([1.0, 0.5, 1.0]))))),
            structure_hash(&dag(Some(("tint", Value::Vector3([1.0, 1.0, 1.0]))))),
            structure_hash(&dag(Some(("tant", Value::Color3([1.0, 1.0, 1.0]))))),
        ];
        for i in 0..hashes.len() {
            for j in i + 1..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "{i} vs {j}");
            }
        }
        let again = structure_hash(&dag(Some(("tint", Value::Color3([1.0, 0.5, 1.0])))));
        assert_eq!(hashes[2], again);
    }

    fn sample_dag(grad: Option<[usize; 2]>) -> Dag {
        let mut dag = Dag::new("m");
        let texture = dag.add_texture(TextureDesc {
            file: "a.png".into(),
            colorspace: None,
            address_u: AddressMode::Periodic,
            address_v: AddressMode::Periodic,
            filter: FilterMode::Linear,
        });
        let read = ContextRead::TexCoord { index: 0, ty: Type::Vector2 };
        let uv = dag.add(Node { kind: NodeKind::Context(read), ty: Type::Vector2, origin: String::new() });
        let du = dag.add(Node {
            kind: NodeKind::Op { op: Op::Dpdx, args: vec![uv] },
            ty: Type::Vector2,
            origin: String::new(),
        });
        let ids = [uv, du];
        let grad = grad.map(|[x, y]| [ids[x], ids[y]]);
        let kind = NodeKind::Sample { texture, uv, default: Value::Float(0.0), grad };
        let s = dag.add(Node { kind, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: s });
        dag
    }

    /// Whether a sample has gradients, and which, is structure.
    #[test]
    fn sample_gradients_change_the_hash() {
        let hashes: Vec<String> = [None, Some([0, 0]), Some([1, 1]), Some([0, 1])]
            .into_iter()
            .map(|g| structure_hash(&sample_dag(g)))
            .collect();
        for i in 0..hashes.len() {
            for j in i + 1..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "{i} vs {j}");
            }
        }
    }

    /// Two DAGs sampling the same files, registered in opposite orders: the shaders bind them to
    /// different slots, so the hashes must differ.
    #[test]
    fn texture_registration_order_changes_the_hash() {
        let dag = |files: [&str; 2]| {
            let mut dag = Dag::new("m");
            let ids = files.map(|file| {
                dag.add_texture(TextureDesc {
                    file: file.into(),
                    colorspace: None,
                    address_u: AddressMode::Periodic,
                    address_v: AddressMode::Periodic,
                    filter: FilterMode::Linear,
                })
            });
            let read = ContextRead::TexCoord { index: 0, ty: Type::Vector2 };
            let uv = dag.add(Node { kind: NodeKind::Context(read), ty: Type::Vector2, origin: String::new() });
            // Sample "a.png" first, then "b.png", whatever their indices.
            let order = if files[0] == "a.png" { [ids[0], ids[1]] } else { [ids[1], ids[0]] };
            for (i, texture) in order.into_iter().enumerate() {
                let kind = NodeKind::Sample { texture, uv, default: Value::Float(0.0), grad: None };
                let s = dag.add(Node { kind, ty: Type::Float, origin: String::new() });
                dag.surface.fields.push(SurfaceField { name: format!("f{i}"), ty: Type::Float, node: s });
            }
            dag
        };
        assert_ne!(structure_hash(&dag(["a.png", "b.png"])), structure_hash(&dag(["b.png", "a.png"])));
    }
}
