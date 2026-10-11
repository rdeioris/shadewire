use shadewire_ir::{Dag, Reflection};

use super::bindings::texture_bindings;
use super::structs::model_struct;
use super::uniforms::uniform_block;
use super::EmitOptions;

/// Joins the sections, skipping empty ones, one blank line between them.
/// Task 20 fills `entry_section` with the expression bodies and the entry function.
pub fn assemble(dag: &Dag, reflection: &Reflection, _options: &EmitOptions) -> String {
    let sections = [model_struct(dag), uniform_block(reflection), texture_bindings(reflection), entry_section()];
    let parts: Vec<&str> = sections.iter().map(|s| s.trim_end()).filter(|s| !s.is_empty()).collect();
    let mut out = parts.join("\n\n");
    out.push('\n');
    out
}

/// Empty until Task 20.
fn entry_section() -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_ir::*;

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
