use shadewire_ir::{Dag, Reflection};

use super::bindings::texture_bindings;
use super::ops::cast;
use super::expressions::{node_lets, node_name};
use super::names::struct_name;
use super::structs::model_struct;
use super::uniforms::uniform_block;
use super::EmitOptions;

/// Joins the sections, skipping empty ones, one blank line between them.
pub fn assemble(dag: &Dag, reflection: &Reflection, options: &EmitOptions) -> String {
    let sections = [model_struct(dag), uniform_block(reflection), texture_bindings(reflection), entry(dag, reflection, options)];
    let parts: Vec<&str> = sections.iter().map(|s| s.trim_end()).filter(|s| !s.is_empty()).collect();
    let mut out = parts.join("\n\n");
    out.push('\n');
    out
}

/// `fn sw_eval_surface() -> {Model}`: the live node lets, then the struct in field order.
pub fn entry(dag: &Dag, reflection: &Reflection, options: &EmitOptions) -> String {
    let name = struct_name(&dag.surface.model);
    let mut out = format!("fn sw_eval_surface() -> {name} {{\n");
    for line in node_lets(dag, reflection, options) {
        out.push_str(&format!("    {line}\n"));
    }
    let fields: Vec<String> = dag
        .surface
        .fields
        .iter()
        .map(|f| cast(&node_name(f.node), dag.node(f.node).ty, f.ty))
        .collect();
    out.push_str(&format!("    return {name}({});\n}}\n", fields.join(", ")));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_ir::*;

    #[test]
    fn entry_returns_fields_in_struct_order() {
        let mut dag = Dag::new("gltf_pbr");
        let c = dag.add(Node { kind: NodeKind::Const(Value::Float(0.5)), ty: Type::Float, origin: String::new() });
        let v = dag.add(Node { kind: NodeKind::Const(Value::Vector3([1.0, 0.0, 0.0])), ty: Type::Vector3, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: v });
        dag.surface.fields.push(SurfaceField { name: "roughness".into(), ty: Type::Float, node: c });
        dag.surface.fields.push(SurfaceField { name: "tint".into(), ty: Type::Color3, node: c });
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let r = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info);
        assert_eq!(
            entry(&dag, &r, &EmitOptions::default()),
            "fn sw_eval_surface() -> GltfPbr {
    let n0: f32 = 0.5;
    let n1: vec3<f32> = vec3<f32>(1.0, 0.0, 0.0);
    return GltfPbr(n1, n0, vec3<f32>(n0));
}
"
        );
    }

    #[test]
    fn skeleton_parses_in_naga() {
        let mut dag = Dag::new("open_pbr_surface");
        let kind = NodeKind::Uniform { name: "w".into(), value: Value::Float(1.0) };
        let u = dag.add(Node { kind, ty: Type::Float, origin: String::new() });
        dag.surface.fields.push(SurfaceField { name: "base_weight".into(), ty: Type::Float, node: u });
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        let r = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info);
        let text = crate::emit(&dag, &r, &EmitOptions::default());
        assert!(text.starts_with("struct OpenPbrSurface {"), "{text}");
        assert!(text.contains("var<uniform> sw_u"), "{text}");
        naga::front::wgsl::parse_str(&text).unwrap_or_else(|e| panic!("{text}: {e:?}"));
    }
}
