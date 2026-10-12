use shadewire::Reflection;

/// One texture and one sampler per live texture, numbered by the Dag texture index.
pub fn texture_bindings(reflection: &Reflection) -> String {
    let mut out = String::new();
    for t in &reflection.textures {
        out.push_str(&format!(
            "@group({}) @binding({}) var shadewire_t{}: texture_2d<f32>;\n",
            reflection.group, t.binding, t.index
        ));
        out.push_str(&format!(
            "@group({}) @binding({}) var shadewire_s{}: sampler;\n",
            reflection.group, t.sampler_binding, t.index
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::*;

    #[test]
    fn uses_dag_index_and_reflection_bindings() {
        let mut dag = Dag::new("m");
        let mk = |f: &str| TextureDesc {
            file: f.into(),
            colorspace: None,
            address_u: AddressMode::Clamp,
            address_v: AddressMode::Clamp,
            filter: FilterMode::Linear,
        };
        dag.add_texture(mk("dead.png"));
        let live = dag.add_texture(mk("a.png"));
        let uv =
            dag.add(Node { kind: NodeKind::Const(Value::Vector2([0.0; 2])), ty: Type::Vector2, origin: String::new() });
        let kind = NodeKind::Sample { texture: live, uv, default: Value::Float(0.0), grad: None };
        let s = dag.add(Node { kind, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "f".into(), ty: Type::Float, node: s });
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let r = reflect(&dag, &BindingOptions { group: 1, first_binding: 4, reserve_uniform_binding: false }, &info)
            .unwrap();
        assert_eq!(
            texture_bindings(&r),
            "@group(1) @binding(4) var shadewire_t1: texture_2d<f32>;\n@group(1) @binding(5) var shadewire_s1: sampler;\n"
        );
    }
}
