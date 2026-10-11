use shadewire_ir::{Reflection, Type};

use super::names::ident;
use super::types::wgsl_type;

/// Booleans are stored as u32 in the block, everything else as its WGSL type.
fn member_type(ty: Type) -> &'static str {
    match ty {
        Type::Boolean => "u32",
        other => wgsl_type(other),
    }
}

/// The uniform struct and its binding; empty when no uniform is live.
pub fn uniform_block(reflection: &Reflection) -> String {
    let Some(binding) = reflection.uniform_binding else {
        return String::new();
    };
    let mut out = String::from("struct MaterialUniforms {\n");
    for u in &reflection.uniforms {
        out.push_str(&format!("    {}: {},\n", ident(&u.name), member_type(u.ty)));
    }
    out.push_str("}\n\n");
    out.push_str(&format!("@group({}) @binding({}) var<uniform> sw_u: MaterialUniforms;\n", reflection.group, binding));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_ir::{reflect, BindingOptions, Dag, Node, NodeKind, SourceInfo, SurfaceField, Value};

    fn reflection_with(uniforms: &[(&str, Value)], group: u32, first: u32) -> Reflection {
        let mut dag = Dag::new("m");
        for (name, value) in uniforms {
            let ty = value.ty();
            let kind = NodeKind::Uniform { name: name.to_string(), value: value.clone() };
            let id = dag.add(Node { kind, ty, origin: String::new() });
            dag.surface.fields.push(SurfaceField { name: name.to_string(), ty, node: id });
        }
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        reflect(&dag, &BindingOptions { group, first_binding: first }, &info)
    }

    #[test]
    fn tint_and_coat_weight() {
        let r = reflection_with(&[("tint", Value::Color3([1.0; 3])), ("coat_weight", Value::Float(0.25))], 2, 0);
        assert_eq!(
            uniform_block(&r),
            "struct MaterialUniforms {\n    tint: vec3<f32>,\n    coat_weight: f32,\n}\n\n@group(2) @binding(0) var<uniform> sw_u: MaterialUniforms;\n"
        );
    }

    #[test]
    fn booleans_are_u32_integers_i32() {
        let r = reflection_with(&[("flag", Value::Boolean(true)), ("n", Value::Integer(2))], 0, 3);
        let text = uniform_block(&r);
        assert!(text.contains("    flag: u32,\n    n: i32,\n"), "{text}");
        assert!(text.contains("@group(0) @binding(3)"), "{text}");
    }

    #[test]
    fn no_uniforms_no_block() {
        assert_eq!(uniform_block(&reflection_with(&[], 0, 0)), "");
    }
}
