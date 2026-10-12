//! A complete, simple host for generated WGSL, used by tests and as a template.
//!
//! Every host function the reflection names returns a private variable (zero unless the
//! entry sets it); transforms are identities. Geomprops start at their authored default
//! when reflection has one, else zero.

use shadewire::{ContextEntry, HOST_PREFIX, Reflection, Type, Value};

use crate::emit::names::ident;
use crate::emit::types::{literal, wgsl_type};

/// One host read: the function name and the type it returns.
struct HostRead {
    function: String,
    ty: Type,
    /// The starting value of the private variable; zero when absent.
    init: Option<Value>,
}

fn named_read(entry: &ContextEntry) -> HostRead {
    HostRead { function: format!("{HOST_PREFIX}{}", entry.name), ty: entry.ty, init: None }
}

fn varyings(reflection: &Reflection) -> Vec<HostRead> {
    reflection.context.varyings.iter().map(named_read).collect()
}

fn all_reads(reflection: &Reflection) -> Vec<HostRead> {
    let context = &reflection.context;
    let mut out: Vec<HostRead> = Vec::new();
    for entry in context.varyings.iter().chain(&context.derived).chain(&context.globals) {
        out.push(named_read(entry));
    }
    for g in &context.geomprops {
        out.push(HostRead {
            function: g.function.clone(),
            ty: g.ty,
            init: g.default.clone().filter(|d| d.ty() == g.ty),
        });
    }
    out
}

/// The first color3 field: `base_color` when there is one, else the first by name.
fn color_field(reflection: &Reflection) -> Option<String> {
    let colors: Vec<&String> = reflection.params.iter().filter(|(_, p)| p.ty == Type::Color3).map(|(n, _)| n).collect();
    colors.iter().find(|n| n.as_str() == "base_color").or(colors.first()).map(|n| ident(n))
}

/// `position_world` for `shadewire_host_position_world`: the fragment input member; the private
/// variable is `shadewire_in_` + this.
fn member_name(function: &str) -> &str {
    function.strip_prefix(HOST_PREFIX).unwrap_or(function)
}

fn fragment_entry(reflection: &Reflection, model_struct: &str) -> String {
    let inputs = varyings(reflection);
    let mut out = String::new();
    let parameter = if inputs.is_empty() {
        "@builtin(position) position: vec4<f32>"
    } else {
        out.push_str("struct ShadewireFragmentIn {\n");
        for (i, v) in inputs.iter().enumerate() {
            out.push_str(&format!("    @location({i}) {}: {},\n", member_name(&v.function), wgsl_type(v.ty)));
        }
        out.push_str("}\n\n");
        "input: ShadewireFragmentIn"
    };
    out.push_str(&format!("@fragment\nfn shadewire_reference_main({parameter}) -> @location(0) vec4<f32> {{\n"));
    for v in &inputs {
        let member = member_name(&v.function);
        out.push_str(&format!("    shadewire_in_{member} = input.{member};\n"));
    }
    for entry in &reflection.context.derived {
        if entry.name.starts_with("view_direction_") {
            out.push_str(&format!("    shadewire_in_{} = vec3<f32>(0.0, 0.0, -1.0);\n", entry.name));
        }
    }
    out.push_str(&format!("    let s: {model_struct} = shadewire_eval_surface();\n"));
    let color = color_field(reflection).map_or("vec3<f32>(0.0)".to_string(), |f| format!("s.{f}"));
    out.push_str(&format!("    return vec4<f32>({color}, 1.0);\n}}\n"));
    out
}

/// Host prelude for `link`. `model_struct` is the WGSL name of the surface struct.
pub fn prelude(reflection: &Reflection, model_struct: &str) -> String {
    let mut out = String::new();
    for r in all_reads(reflection) {
        let ty = wgsl_type(r.ty);
        let var = format!("shadewire_in_{}", member_name(&r.function));
        let start = r.init.as_ref().map_or(String::new(), |v| format!(" = {}", literal(v)));
        out.push_str(&format!("var<private> {var}: {ty}{start};\nfn {}() -> {ty} {{ return {var}; }}\n", r.function));
    }
    for t in &reflection.context.transforms {
        out.push_str(&format!("fn {HOST_PREFIX}{t}(v: vec3<f32>) -> vec3<f32> {{ return v; }}\n"));
    }
    out.push('\n');
    out.push_str(&fragment_entry(reflection, model_struct));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::{add, reflection};
    use crate::{EmitOptions, emit, link::link};
    use shadewire::*;

    #[test]
    fn no_varyings_takes_the_position_builtin() {
        let mut dag = Dag::new("gltf_pbr");
        let c = add(&mut dag, NodeKind::Const(Value::Float(0.5)), Type::Float);
        dag.surface.fields.push(SurfaceField { name: "roughness".into(), ty: Type::Float, node: c });
        let r = reflection(&dag);
        let prelude = prelude(&r, "GltfPbr");
        assert!(prelude.contains("@builtin(position)"));
        assert!(prelude.contains("vec3<f32>(0.0), 1.0"));
        link(&emit(&dag, &r, &EmitOptions::default()), &prelude).unwrap();
    }

    #[test]
    fn transform_host_functions_are_pinned() {
        let mut dag = Dag::new("gltf_pbr");
        let n = add(&mut dag, NodeKind::Context(ContextRead::Normal(Space::Object)), Type::Vector3);
        let t = add(
            &mut dag,
            NodeKind::Op {
                op: Op::Transform { kind: TransformKind::Normal, from: Space::Object, to: Space::World },
                args: vec![n],
            },
            Type::Vector3,
        );
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: t });
        let r = reflection(&dag);
        let generated = emit(&dag, &r, &EmitOptions::default());
        assert!(generated.contains("shadewire_host_transform_normal_object_to_world("), "{generated}");
        let prelude = prelude(&r, "GltfPbr");
        assert!(prelude.contains("fn shadewire_host_transform_normal_object_to_world(v: vec3<f32>) -> vec3<f32>"));
        link(&generated, &prelude).unwrap();
    }

    #[test]
    fn geomprops_start_at_their_authored_default() {
        let mut dag = Dag::new("gltf_pbr");
        let g = add(
            &mut dag,
            NodeKind::Context(ContextRead::GeomProp { name: "tint".into(), ty: Type::Color3 }),
            Type::Color3,
        );
        let z = add(
            &mut dag,
            NodeKind::Context(ContextRead::GeomProp { name: "bare".into(), ty: Type::Float }),
            Type::Float,
        );
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: g });
        dag.surface.fields.push(SurfaceField { name: "roughness".into(), ty: Type::Float, node: z });
        dag.geomprop_defaults.insert("tint".into(), Value::Color3([1.0, 0.5, 0.25]));
        let r = reflection(&dag);
        let prelude = prelude(&r, "GltfPbr");
        assert!(
            prelude.contains("var<private> shadewire_in_geomprop_tint: vec3<f32> = vec3<f32>(1.0, 0.5, 0.25);"),
            "{prelude}"
        );
        assert!(prelude.contains("var<private> shadewire_in_geomprop_bare: f32;"), "{prelude}");
        link(&emit(&dag, &r, &EmitOptions::default()), &prelude).unwrap();
    }

    #[test]
    fn geomprop_and_view_direction_get_functions() {
        let mut dag = Dag::new("gltf_pbr");
        let g = add(
            &mut dag,
            NodeKind::Context(ContextRead::GeomProp { name: "my prop".into(), ty: Type::Float }),
            Type::Float,
        );
        let v = add(&mut dag, NodeKind::Context(ContextRead::ViewDirection(Space::World)), Type::Vector3);
        let n = add(&mut dag, NodeKind::Context(ContextRead::Normal(Space::World)), Type::Vector3);
        dag.surface.fields.push(SurfaceField { name: "roughness".into(), ty: Type::Float, node: g });
        dag.surface.fields.push(SurfaceField { name: "base_color".into(), ty: Type::Color3, node: v });
        dag.surface.fields.push(SurfaceField { name: "emissive".into(), ty: Type::Color3, node: n });
        let r = reflection(&dag);
        let prelude = prelude(&r, "GltfPbr");
        assert!(prelude.contains("fn shadewire_host_geomprop_my_prop() -> f32"));
        assert!(prelude.contains("shadewire_in_view_direction_world = vec3<f32>(0.0, 0.0, -1.0);"));
        assert!(prelude.contains("@location(0) normal_world: vec3<f32>"));
        link(&emit(&dag, &r, &EmitOptions::default()), &prelude).unwrap();
    }
}
