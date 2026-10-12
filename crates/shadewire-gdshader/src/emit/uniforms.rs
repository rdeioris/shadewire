use shadewire::{Reflection, Type};

use super::names::UniformNames;
use super::sample::texture_uniform;
use super::types::{gd_type, is_constant_literal, literal};

/// Colors keep the color picker without Godot's sRGB-to-linear conversion on upload:
/// authored values are already in the working colorspace.
fn hints(ty: Type) -> &'static str {
    match ty {
        Type::Color3 | Type::Color4 => " : source_color, color_conversion_disabled",
        _ => "",
    }
}

/// The live uniforms and textures in one inspector group; empty when there are none.
/// Godot binds by name: uniforms take `names`, textures `shadewire_t<index>`.
pub fn uniform_block(reflection: &Reflection, names: &UniformNames) -> String {
    if reflection.uniforms.is_empty() && reflection.textures.is_empty() {
        return String::new();
    }
    let mut out = String::from("group_uniforms material;\n");
    for (i, u) in reflection.uniforms.iter().enumerate() {
        // A default must be a constant expression; inf and nan are not.
        let default =
            if is_constant_literal(&u.default) { format!(" = {}", literal(&u.default)) } else { String::new() };
        out.push_str(&format!("uniform {} {}{}{default};\n", gd_type(u.ty), names.name(i), hints(u.ty)));
    }
    for t in &reflection.textures {
        out.push_str(&texture_uniform(t));
    }
    out.push_str("group_uniforms;\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shadewire::{BindingOptions, Dag, Node, NodeKind, SourceInfo, SurfaceField, Value, reflect};

    fn reflection_with(uniforms: &[(&str, Value)]) -> Reflection {
        let mut dag = Dag::new("m");
        for (name, value) in uniforms {
            let ty = value.ty();
            let kind = NodeKind::Uniform { name: name.to_string(), value: value.clone() };
            let id = dag.add(Node { kind, ty, origin: String::new() });
            dag.surface.fields.push(SurfaceField { name: name.to_string(), ty, node: id });
        }
        let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
        reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, &info).unwrap()
    }

    #[test]
    fn hints_defaults_and_native_types() {
        let r = reflection_with(&[
            ("tint", Value::Color3([1.0, 0.5, 0.25])),
            ("coat", Value::Float(0.25)),
            ("flag", Value::Boolean(true)),
            ("count", Value::Integer(-2)),
            ("bad", Value::Float(f32::INFINITY)),
        ]);
        assert_eq!(
            uniform_block(&r, &UniformNames::new(&r)),
            "group_uniforms material;
uniform vec3 tint : source_color, color_conversion_disabled = vec3(1.0, 0.5, 0.25);
uniform float coat = 0.25;
uniform bool flag = true;
uniform int count = -2;
uniform float bad;
group_uniforms;
"
        );
    }

    #[test]
    fn colliding_and_reserved_names_move() {
        let r = reflection_with(&[
            ("my-prop", Value::Float(1.0)),
            ("my_prop", Value::Float(2.0)),
            ("uniform", Value::Float(3.0)),
            ("jit", Value::Float(4.0)),
            ("n3", Value::Float(5.0)),
            ("shadewire_t0", Value::Float(6.0)),
        ]);
        let names = UniformNames::new(&r);
        let all: Vec<&str> = (0..6).map(|i| names.name(i)).collect();
        assert_eq!(all, ["my_prop", "my_prop_2", "uniform_", "jit_", "n3_", "shadewire_t0_"]);
    }

    #[test]
    fn nothing_live_no_block() {
        let r = reflection_with(&[]);
        assert_eq!(uniform_block(&r, &UniformNames::new(&r)), "");
    }
}
