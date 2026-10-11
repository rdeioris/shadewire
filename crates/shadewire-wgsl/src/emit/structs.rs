use shadewire_ir::Dag;

use super::names::{ident, struct_name};
use super::types::wgsl_type;

/// The struct the entry function returns: one field per surface field.
pub fn model_struct(dag: &Dag) -> String {
    let mut out = format!("struct {} {{\n", struct_name(&dag.surface.model));
    for field in &dag.surface.fields {
        out.push_str(&format!("    {}: {},\n", ident(&field.name), wgsl_type(field.ty)));
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire_ir::{NodeId, SurfaceField, Type};

    #[test]
    fn two_fields() {
        let mut dag = Dag::new("open_pbr_surface");
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: NodeId(0) });
        dag.surface.fields.push(SurfaceField { name: "filter".into(), ty: Type::Float, node: NodeId(1) });
        assert_eq!(
            model_struct(&dag),
            "struct OpenPbrSurface {\n    base_color: vec3<f32>,\n    filter_: f32,\n}\n"
        );
    }
}
