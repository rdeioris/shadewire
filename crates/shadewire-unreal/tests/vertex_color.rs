//! Unreal's VertexColor reads the vertex buffer's bytes, which hold the sRGB encoding of the mesh's linear colours,
//! the colours geomcolor reads: a material that reads it is the same as one that reads the encoded probe colour.

use serde_json::{Value, json};
use shadewire::testing::assert_same_evaluation;
use shadewire_unreal::{IrOptions, to_ir};

/// The colour `shadewire::testing::Probe` gives every geomcolor read.
const PROBE: [f32; 4] = [0.2, 0.4, 0.6, 1.0];

fn encode(c: f32) -> f32 {
    if c > 0.003_130_8 { 1.055 * c.powf(1.0 / 2.4) - 0.055 } else { 12.92 * c }
}

fn masks(masks: &[[u8; 4]]) -> Value {
    Value::Array(masks.iter().map(|m| json!({ "name": "", "mask": m })).collect())
}

/// `pins` are (pin, expression, output, the output's channel mask): a link carries the mask of the output it reads.
fn document(expressions: Vec<Value>, pins: &[(&str, &str, usize, Option<[u8; 4]>)]) -> String {
    let pins: Vec<Value> = pins
        .iter()
        .map(|(pin, from, output, mask)| json!({ "pin": pin, "from": [from, output], "mask": mask }))
        .collect();
    json!({
        "format": "shadewire-unreal-graph",
        "version": 1,
        "producer": "test",
        "substrate": false,
        "materials": [{
            "path": "/Game/Test/M_Test.M_Test", "name": "M_Test", "class": "Material", "parent": null,
            "settings": {
                "MaterialDomain": "MD_Surface", "BlendMode": "BLEND_Opaque", "ShadingModel": "MSM_DefaultLit",
                "TwoSided": false, "bTangentSpaceNormal": true, "bUseMaterialAttributes": false,
                "OpacityMaskClipValue": 0.3333, "TranslucencyLightingMode": "TLM_VolumetricNonDirectional",
                "bIsThinSurface": false, "ShadingModels": ["MSM_DefaultLit"]
            },
            "pins": pins,
            "expressions": expressions
        }],
        "parents": [],
        "functions": [],
        "textures": []
    })
    .to_string()
}

#[test]
fn vertex_color_is_the_srgb_encoding_of_the_linear_geomcolor() {
    let rgba = [[1, 1, 1, 0], [1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]];
    let vertex_color = document(
        vec![json!({
            "name": "VertexColor", "class": "MaterialExpressionVertexColor", "properties": {},
            "inputs": [], "outputs": masks(&rgba)
        })],
        &[("BaseColor", "VertexColor", 0, Some(rgba[0])), ("Roughness", "VertexColor", 4, Some(rgba[4]))],
    );
    let encoded = [encode(PROBE[0]), encode(PROBE[1]), encode(PROBE[2]), 1.0];
    let constants = document(
        vec![
            json!({
                "name": "Color", "class": "MaterialExpressionConstant3Vector", "properties": { "Constant": encoded },
                "inputs": [], "outputs": masks(&rgba[..4])
            }),
            json!({
                "name": "Alpha", "class": "MaterialExpressionConstant", "properties": { "R": PROBE[3] },
                "inputs": [], "outputs": masks(&[[0, 0, 0, 0]])
            }),
        ],
        &[("BaseColor", "Color", 0, Some(rgba[0])), ("Roughness", "Alpha", 0, None)],
    );
    let options = IrOptions::default();
    let vertex_color = to_ir(&vertex_color, None, &options).unwrap();
    let constants = to_ir(&constants, None, &options).unwrap();
    assert!(vertex_color.warnings.is_empty(), "{:?}", vertex_color.warnings);
    assert_same_evaluation(&vertex_color.dag, &constants.dag, "VertexColor");
    // and the encoding is the real one: bytes 203 and 51 decode to these linear values, and encode back to them
    assert!((encode(0.597_201_76) * 255.0 - 203.0).abs() < 0.01);
    assert!((encode(0.033_104_766) * 255.0 - 51.0).abs() < 0.01);
}
