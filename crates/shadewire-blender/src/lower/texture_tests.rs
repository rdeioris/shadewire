use shadewire::{AddressMode, ContextRead, FilterMode, NodeKind, Value};

use super::tests::{close, eval_field, lower, lower_field};

const IMAGE_DOC_TAIL: &str = r#""images": [{"name": "albedo", "filepath": "//textures/albedo.png", "colorspace": "sRGB", "alpha_mode": "STRAIGHT", "source": "FILE", "packed": false}]"#;

fn image_node(projection: &str, interpolation: &str, extension: &str) -> String {
    format!(
        r#"{{"name": "Img", "type": "ShaderNodeTexImage", "mute": false,
      "properties": {{"image": "albedo", "projection": "{projection}", "interpolation": "{interpolation}", "projection_blend": 0.0, "extension": "{extension}"}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [0.0, 0.0, 0.0]}}],
      "outputs": [{{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true}}, {{"identifier": "Alpha", "name": "Alpha", "type": "VALUE", "enabled": true}}], "internal_links": []}}"#
    )
}

/// `lower` with the albedo image in the document's image list.
fn lower_with_image(nodes: &str, links: &str) -> (shadewire::Dag, Vec<String>) {
    let text = super::tests::doc(nodes, links).replace(r#""images": []"#, IMAGE_DOC_TAIL);
    let options = shadewire_nodes::IrOptions { values_as_uniforms: false, ..Default::default() };
    let ir = crate::to_ir(&text, None, &options).unwrap_or_else(|e| panic!("{e}"));
    (ir.dag, ir.warnings)
}

#[test]
fn an_image_texture_is_one_texture_with_its_colorspace_modes_and_both_outputs() {
    let links =
        r#"[[["Img", "Color"], ["Principled BSDF", "Base Color"]], [["Img", "Alpha"], ["Principled BSDF", "Alpha"]]]"#;
    let (dag, warnings) = lower_with_image(&image_node("FLAT", "Closest", "EXTEND"), links);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(dag.textures.len(), 1);
    let t = &dag.textures[0];
    assert_eq!((t.file.as_str(), t.colorspace.as_deref()), ("//textures/albedo.png", Some("srgb_texture")));
    assert_eq!((t.address_u, t.address_v, t.filter), (AddressMode::Clamp, AddressMode::Clamp, FilterMode::Closest));
    assert_eq!(
        dag.nodes.iter().filter(|n| matches!(n.kind, NodeKind::Sample { .. })).count(),
        1,
        "one sample feeds both outputs"
    );
    // The sample's uv is the first UV set, and the alpha is channel 3: at every probe the alpha ProbeTextures gives is 1.
    assert!(dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Context(ContextRead::TexCoord { index: 0, .. }))));
    assert_eq!(eval_field(&dag, "geometry_opacity", 0), Value::Float(1.0));
    let (_, warnings) = lower_with_image(&image_node("BOX", "Smart", "CLIP"), links);
    assert_eq!(warnings, [r#"ShaderNodeTexImage "Img": projection BOX read as Flat"#]);
    let (dag, _) = lower_with_image(&image_node("FLAT", "Smart", "CLIP"), links);
    assert_eq!((dag.textures[0].address_u, dag.textures[0].filter), (AddressMode::Constant, FilterMode::Cubic));
    let (dag, warnings) = lower(&image_node("FLAT", "Linear", "REPEAT"), links);
    assert!(dag.textures.is_empty());
    assert_eq!(warnings, [r#"ShaderNodeTexImage "Img": has no image, black"#]);
}

#[test]
fn texture_coordinate_and_geometry_are_context_reads() {
    let coord = r#"{"name": "T", "type": "ShaderNodeTexCoord", "mute": false, "properties": {"object": null, "from_instancer": false}, "inputs": [],
      "outputs": [{"identifier": "UV", "name": "UV", "type": "VECTOR", "enabled": true, "value": [0,0,0]}, {"identifier": "Object", "name": "Object", "type": "VECTOR", "enabled": true, "value": [0,0,0]},
                  {"identifier": "Camera", "name": "Camera", "type": "VECTOR", "enabled": true, "value": [0,0,0]}], "internal_links": []}"#;
    let (dag, warnings) = lower(coord, r#"[[["T", "UV"], ["Principled BSDF", "Base Color"]]]"#);
    assert!(warnings.is_empty(), "{warnings:?}");
    // Probe 1 reads uv (0.35, 0.65): the color is (u, v, 0).
    assert!(close(&eval_field(&dag, "base_color", 1), &Value::Color3([0.35, 0.65, 0.0])));
    let (dag, _) = lower(coord, r#"[[["T", "Object"], ["Principled BSDF", "Base Color"]]]"#);
    assert!(close(&eval_field(&dag, "base_color", 0), &Value::Color3([0.1, 0.3, 0.7])));
    let (dag, warnings) = lower(coord, r#"[[["T", "Camera"], ["Principled BSDF", "Base Color"]]]"#);
    assert_eq!(eval_field(&dag, "base_color", 0), Value::Color3([0.0; 3]));
    assert_eq!(warnings, [r#"ShaderNodeTexCoord "T": Camera not available, zero"#]);
    let geometry = r#"{"name": "G", "type": "ShaderNodeNewGeometry", "mute": false, "properties": {}, "inputs": [],
      "outputs": [{"identifier": "Position", "name": "Position", "type": "VECTOR", "enabled": true, "value": [0,0,0]}, {"identifier": "Incoming", "name": "Incoming", "type": "VECTOR", "enabled": true, "value": [0,0,0]},
                  {"identifier": "Backfacing", "name": "Backfacing", "type": "VALUE", "enabled": true, "value": 0.0}], "internal_links": []}"#;
    let (dag, _) = lower(geometry, r#"[[["G", "Incoming"], ["Principled BSDF", "Base Color"]]]"#);
    assert!(dag.nodes.iter().any(|n| matches!(n.kind, NodeKind::Context(ContextRead::ViewDirection(_)))));
    // The probe's view direction runs camera to surface, (0, 0, 1); Incoming runs surface to camera.
    let incoming = eval_field(&dag, "base_color", 0);
    assert!(close(&incoming, &Value::Color3([0.0, 0.0, -1.0])), "{incoming:?}");
    let (_, warnings) = lower(geometry, r#"[[["G", "Backfacing"], ["Principled BSDF", "Roughness"]]]"#);
    assert_eq!(warnings, [r#"ShaderNodeNewGeometry "G": Backfacing not available, zero"#]);
}

#[test]
fn attributes_and_color_attributes_are_geometric_properties() {
    let attribute = r#"{"name": "A", "type": "ShaderNodeAttribute", "mute": false, "properties": {"attribute_type": "GEOMETRY", "attribute_name": "mask"}, "inputs": [],
      "outputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0,0,0,1]}, {"identifier": "Fac", "name": "Fac", "type": "VALUE", "enabled": true, "value": 0.0}], "internal_links": []}"#;
    let (dag, warnings) = lower(attribute, r#"[[["A", "Fac"], ["Principled BSDF", "Roughness"]]]"#);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(dag.nodes.iter().any(|n| matches!(&n.kind, NodeKind::Context(ContextRead::GeomProp { name, ty: shadewire::Type::Float }) if name == "mask")));
    let vertex = r#"{"name": "VC", "type": "ShaderNodeVertexColor", "mute": false, "properties": {"layer_name": "Col"}, "inputs": [],
      "outputs": [{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0,0,0,1]}, {"identifier": "Alpha", "name": "Alpha", "type": "VALUE", "enabled": true, "value": 0.0}], "internal_links": []}"#;
    let (dag, warnings) = lower(
        vertex,
        r#"[[["VC", "Color"], ["Principled BSDF", "Base Color"]], [["VC", "Alpha"], ["Principled BSDF", "Alpha"]]]"#,
    );
    assert_eq!(warnings, [r#"ShaderNodeVertexColor "VC": layer "Col" read as the first color set"#]);
    assert!(close(&eval_field(&dag, "base_color", 0), &Value::Color3([0.2, 0.4, 0.6])));
    assert_eq!(eval_field(&dag, "geometry_opacity", 0), Value::Float(1.0));
}

#[test]
fn normal_map_bump_and_tangent_perturb_the_shading_normal() {
    let normal_map = |space: &str| {
        format!(
            r#"{{"name": "NM", "type": "ShaderNodeNormalMap", "mute": false, "properties": {{"space": "{space}", "uv_map": ""}},
      "inputs": [{{"identifier": "Strength", "name": "Strength", "type": "VALUE", "enabled": true, "value": 1.0}}, {{"identifier": "Color", "name": "Color", "type": "RGBA", "enabled": true, "value": [0.5, 0.5, 1.0, 1.0]}}],
      "outputs": [{{"identifier": "Normal", "name": "Normal", "type": "VECTOR", "enabled": true}}], "internal_links": []}}"#
        )
    };
    let (dag, warnings) = lower(&normal_map("TANGENT"), r#"[[["NM", "Normal"], ["Principled BSDF", "Normal"]]]"#);
    assert!(warnings.is_empty(), "{warnings:?}");
    // A flat normal map leaves the probe's normal alone.
    let n = eval_field(&dag, "geometry_normal", 2);
    let expected = {
        let v = [0.2f32, 1.0, 0.2];
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        Value::Vector3(v.map(|c| c / l))
    };
    assert!(close(&n, &expected), "{n:?}");
    let (_, warnings) = lower(&normal_map("OBJECT"), r#"[[["NM", "Normal"], ["Principled BSDF", "Normal"]]]"#);
    assert_eq!(warnings, [r#"ShaderNodeNormalMap "NM": OBJECT space read as tangent space"#]);
    let bump = r#"{"name": "B", "type": "ShaderNodeBump", "mute": false, "properties": {"invert": false},
      "inputs": [{"identifier": "Strength", "name": "Strength", "type": "VALUE", "enabled": true, "value": 0.0}, {"identifier": "Distance", "name": "Distance", "type": "VALUE", "enabled": true, "value": 0.2},
                 {"identifier": "Height", "name": "Height", "type": "VALUE", "enabled": true, "value": 1.0}, {"identifier": "Normal", "name": "Normal", "type": "VECTOR", "enabled": true, "value": [0,0,0]}],
      "outputs": [{"identifier": "Normal", "name": "Normal", "type": "VECTOR", "enabled": true}], "internal_links": []}"#;
    let (dag, warnings) = lower(bump, r#"[[["B", "Normal"], ["Principled BSDF", "Normal"]]]"#);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(close(&eval_field(&dag, "geometry_normal", 2), &expected), "a zero-strength bump is the normal");
    let tangent = r#"{"name": "Tg", "type": "ShaderNodeTangent", "mute": false, "properties": {"direction_type": "UV_MAP", "axis": "Z", "uv_map": ""}, "inputs": [],
      "outputs": [{"identifier": "Tangent", "name": "Tangent", "type": "VECTOR", "enabled": true, "value": [0,0,0]}], "internal_links": []}"#;
    let (dag, _) = lower(tangent, r#"[[["Tg", "Tangent"], ["Principled BSDF", "Base Color"]]]"#);
    assert!(close(&eval_field(&dag, "base_color", 0), &Value::Color3([1.0, 0.0, 0.0])));
}

#[test]
fn mapping_transforms_points_and_textures_inversely() {
    let mapping = |kind: &str, vector: [f32; 3]| {
        format!(
            r#"{{"name": "Map", "type": "ShaderNodeMapping", "mute": false, "properties": {{"vector_type": "{kind}"}},
      "inputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true, "value": [{}, {}, {}]}},
                 {{"identifier": "Location", "name": "Location", "type": "VECTOR", "enabled": true, "value": [1.0, 0.0, 0.0]}},
                 {{"identifier": "Rotation", "name": "Rotation", "type": "VECTOR", "enabled": true, "value": [0.0, 0.0, 1.5707963]}},
                 {{"identifier": "Scale", "name": "Scale", "type": "VECTOR", "enabled": true, "value": [2.0, 2.0, 2.0]}}],
      "outputs": [{{"identifier": "Vector", "name": "Vector", "type": "VECTOR", "enabled": true}}], "internal_links": []}}"#,
            vector[0], vector[1], vector[2]
        )
    };
    let link = r#"[[["Map", "Vector"], ["Principled BSDF", "Subsurface Radius"]]]"#;
    // Point: scale (2, 0, 0), rotate 90° about Z counterclockwise (0, 2, 0), translate (1, 2, 0).
    let (value, warnings) = lower_field(&mapping("POINT", [1.0, 0.0, 0.0]), link, "subsurface_radius_scale");
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(close(&value, &Value::Color3([1.0, 2.0, 0.0])), "{value:?}");
    // Texture is the inverse: (1, 2, 0) goes back to (1, 0, 0).
    let (value, _) = lower_field(&mapping("TEXTURE", [1.0, 2.0, 0.0]), link, "subsurface_radius_scale");
    assert!(close(&value, &Value::Color3([1.0, 0.0, 0.0])), "{value:?}");
    let (value, _) = lower_field(&mapping("VECTOR", [1.0, 0.0, 0.0]), link, "subsurface_radius_scale");
    assert!(close(&value, &Value::Color3([0.0, 2.0, 0.0])), "{value:?}");
    let (value, _) = lower_field(&mapping("NORMAL", [1.0, 0.0, 0.0]), link, "subsurface_radius_scale");
    assert!(close(&value, &Value::Color3([0.0, 1.0, 0.0])), "{value:?}");
}
