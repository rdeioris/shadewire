use shadewire::{NodeKind, Value};
use shadewire_nodes::IrOptions;

use super::tests::{close, doc, lower, lower_field};
use crate::to_ir;

const VALUE: &str = r#"{"name": "V", "type": "ShaderNodeValue", "mute": false, "properties": {}, "inputs": [],
  "outputs": [{"identifier": "Value", "name": "Value", "type": "VALUE", "enabled": true, "value": 0.25}], "internal_links": []}"#;

#[test]
fn a_principled_bsdf_alone_is_open_pbr_with_its_values_as_uniforms() {
    let ir = to_ir(&doc(VALUE, "[]"), None, &IrOptions::default()).unwrap();
    assert_eq!(ir.dag.surface.model, "open_pbr_surface");
    assert_eq!(ir.source.source, "blender");
    assert_eq!(ir.source.mtlx_version, "test");
    assert!(ir.warnings.is_empty(), "{:?}", ir.warnings);
    let names: Vec<&str> = ir
        .dag
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Uniform { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(names.contains(&"Principled_BSDF_Base_Color"), "{names:?}");
    assert!(names.contains(&"Principled_BSDF_Roughness"), "{names:?}");
    // The unlinked Normal is not bound: the nodedef's default (the shading normal) stays.
    let normal = ir.dag.surface.fields.iter().find(|f| f.name == "geometry_normal").unwrap();
    assert!(matches!(ir.dag.node(normal.node).kind, NodeKind::Context(_)), "{:?}", ir.dag.node(normal.node).kind);
    // The unused value node V is never visited: no uniform for it.
    assert!(!names.iter().any(|n| n.starts_with("V_")), "{names:?}");
}

#[test]
fn the_field_table_inverts_the_import_side() {
    let principled = |socket: &str, value: &str| {
        super::tests::PRINCIPLED.replace(
            r#"{"identifier": "Metallic", "name": "Metallic", "type": "VALUE", "enabled": true, "value": 0.0}"#,
            &format!(
                r#"{{"identifier": "Metallic", "name": "Metallic", "type": "VALUE", "enabled": true, "value": 0.0}},
                {{"identifier": "{socket}", "name": "{socket}", "type": "VALUE", "enabled": true, "value": {value}}}"#
            ),
        )
    };
    // Specular IOR Level 0.25 → specular_weight 0.5.
    let nodes = principled("Specular IOR Level", "0.25");
    let text = doc(VALUE, "[]").replace(super::tests::PRINCIPLED, &nodes);
    let options = IrOptions { values_as_uniforms: false, ..IrOptions::default() };
    let dag = to_ir(&text, None, &options).unwrap().dag;
    let field = |name: &str| {
        let f = dag.surface.fields.iter().find(|f| f.name == name).unwrap();
        shadewire_nodes::elaborate::fold::fold_constant_of(&dag, f.node).unwrap()
    };
    assert_eq!(field("specular_weight"), Value::Float(0.5));
    // Thin Film Thickness 500 nm → 0.5 µm and a weight of 1; the default 0 → weight 0.
    let text = doc(VALUE, "[]").replace(super::tests::PRINCIPLED, &principled("Thin Film Thickness", "500.0"));
    let dag2 = to_ir(&text, None, &options).unwrap().dag;
    let field2 = |name: &str| {
        let f = dag2.surface.fields.iter().find(|f| f.name == name).unwrap();
        shadewire_nodes::elaborate::fold::fold_constant_of(&dag2, f.node).unwrap()
    };
    assert!(close(&field2("thin_film_thickness"), &Value::Float(0.5)));
    assert_eq!(field2("thin_film_weight"), Value::Float(1.0));
    assert_eq!(field("thin_film_weight"), Value::Float(0.0));
    // Subsurface Radius (1, 0.2, 0.1) is the radius scale, as a color.
    assert!(close(&field("subsurface_radius_scale"), &Value::Color3([1.0, 0.2, 0.1])));
    assert_eq!(field("subsurface_color"), field("base_color"));
}

#[test]
fn a_linked_value_reaches_its_field_through_the_implicit_conversion() {
    let (value, warnings) =
        lower_field(VALUE, r#"[[["V", "Value"], ["Principled BSDF", "Base Color"]]]"#, "base_color");
    assert_eq!(value, Value::Color3([0.25, 0.25, 0.25]));
    assert!(warnings.is_empty(), "{warnings:?}");
    let rgb = r#"{"name": "C", "type": "ShaderNodeRGB", "mute": false, "properties": {}, "inputs": [],
      "outputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [1.0, 0.5, 0.0, 1.0]}], "internal_links": []}"#;
    let (value, _) = lower_field(rgb, r#"[[["C", "Color"], ["Principled BSDF", "Roughness"]]]"#, "specular_roughness");
    assert!(close(&value, &Value::Float(0.2126 + 0.5 * 0.7152)), "{value:?}");
}

#[test]
fn an_unsupported_node_is_its_default_output_with_a_warning() {
    let fresnel = r#"{"name": "F", "type": "ShaderNodeFresnel", "mute": false, "properties": {},
      "inputs": [{"identifier": "IOR", "name": "IOR", "type": "VALUE", "enabled": true, "value": 1.45}],
      "outputs": [{"identifier": "Fac", "name": "Fac", "type": "VALUE", "enabled": true, "value": 0.0}], "internal_links": []}"#;
    let (value, warnings) =
        lower_field(fresnel, r#"[[["F", "Fac"], ["Principled BSDF", "Metallic"]]]"#, "base_metalness");
    assert_eq!(value, Value::Float(0.0));
    assert_eq!(warnings, [r#"ShaderNodeFresnel "F": not lowered, output Fac is its default value"#]);
}

#[test]
fn an_unlinked_surface_and_a_missing_output_are_errors_and_volume_is_a_warning() {
    let text = doc(VALUE, "[]").replace(
        r#"{"from": ["Principled BSDF", "BSDF"], "to": ["Material Output", "Surface"], "muted": false, "valid": true}"#,
        r#"{"from": ["Principled BSDF", "BSDF"], "to": ["Material Output", "Volume"], "muted": false, "valid": true}"#,
    );
    let err = to_ir(&text, None, &IrOptions::default()).unwrap_err();
    assert!(err.to_string().contains("Surface is not linked"), "{err}");
    let text = doc(VALUE, "[]").replace(r#""output": "Material Output""#, r#""output": null"#);
    let err = to_ir(&text, None, &IrOptions::default()).unwrap_err();
    assert!(err.to_string().contains("no active Material Output"), "{err}");
    let (_, warnings) = lower(VALUE, r#"[[["V", "Value"], ["Material Output", "Displacement"]]]"#);
    assert_eq!(warnings, [r#"ShaderNodeOutputMaterial "Material Output": Displacement ignored"#]);
}
