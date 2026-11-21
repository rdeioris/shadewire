//! A material that writes a runtime virtual texture and samples one (a landscape drawn from its own virtual texture)
//! reads back what it writes: the same material as one wired straight to what it writes. And Transform between
//! tangent and world space uses the vertex tangent frame: tangent (0, 0, 1) is the vertex normal.

use serde_json::{Value, json};
use shadewire::testing::assert_same_evaluation;
use shadewire_unreal::{IrOptions, to_ir};

const RGB: [u8; 4] = [1, 1, 1, 0];

fn outputs(n: usize) -> Value {
    Value::Array((0..n).map(|_| json!({ "name": "", "mask": null })).collect())
}

fn input(index: usize, name: &str, from: &str, output: usize, mask: Option<[u8; 4]>) -> Value {
    json!({ "index": index, "name": name, "from": [from, output], "mask": mask })
}

fn constant3(name: &str, value: [f32; 3]) -> Value {
    json!({ "name": name, "class": "MaterialExpressionConstant3Vector",
            "properties": { "Constant": [value[0], value[1], value[2], 1.0] }, "inputs": [], "outputs": outputs(1) })
}

fn constant(name: &str, value: f32) -> Value {
    json!({ "name": name, "class": "MaterialExpressionConstant", "properties": { "R": value }, "inputs": [], "outputs": outputs(1) })
}

/// `pins` are (pin, expression, output, mask).
fn document(expressions: Vec<Value>, pins: &[(&str, &str, usize, Option<[u8; 4]>)]) -> String {
    let pins: Vec<Value> = pins
        .iter()
        .map(|(pin, from, output, mask)| json!({ "pin": pin, "from": [from, output], "mask": mask }))
        .collect();
    json!({
        "format": "shadewire-unreal-graph", "version": 1, "producer": "test", "substrate": false,
        "materials": [{
            "path": "/Game/Test/M_Test.M_Test", "name": "M_Test", "class": "Material", "parent": null,
            "settings": {
                "MaterialDomain": "MD_Surface", "BlendMode": "BLEND_Opaque", "ShadingModel": "MSM_DefaultLit",
                "TwoSided": false, "bTangentSpaceNormal": true, "bUseMaterialAttributes": false,
                "OpacityMaskClipValue": 0.3333, "TranslucencyLightingMode": "TLM_VolumetricNonDirectional",
                "bIsThinSurface": false, "ShadingModels": ["MSM_DefaultLit"]
            },
            "pins": pins, "expressions": expressions
        }],
        "parents": [], "functions": [], "textures": []
    })
    .to_string()
}

fn same(a: &str, b: &str, what: &str) {
    let options = IrOptions::default();
    let a = to_ir(a, None, &options).unwrap();
    let b = to_ir(b, None, &options).unwrap();
    assert_same_evaluation(&a.dag, &b.dag, what);
}

#[test]
fn a_material_sampling_its_own_virtual_texture_reads_what_it_writes() {
    let colour = [0.3, 0.5, 0.2];
    let sample_outputs: Vec<Value> =
        ["BaseColor", "Specular", "Roughness", "Normal", "WorldHeight", "Mask", "Displacement", "Mask4"]
            .iter()
            .map(|n| json!({ "name": n, "mask": null }))
            .collect();
    let sample = json!({
        "name": "Sample", "class": "MaterialExpressionRuntimeVirtualTextureSampleParameter",
        "properties": { "VirtualTexture": "/Game/Test/RVT.RVT", "ParameterName": "RVT" }, "inputs": [],
        "outputs": sample_outputs
    });
    let writer = json!({
        "name": "Write", "class": "MaterialExpressionRuntimeVirtualTextureOutput", "properties": {},
        "inputs": [input(0, "BaseColor", "Colour", 0, Some(RGB)), input(2, "Roughness", "Rough", 0, None)],
        "outputs": []
    });
    let reads = document(
        vec![constant3("Colour", colour), constant("Rough", 0.7), writer, sample],
        &[("BaseColor", "Sample", 0, Some(RGB)), ("Roughness", "Sample", 2, None), ("Metallic", "Sample", 1, None)],
    );
    let direct = document(
        vec![constant3("Colour", colour), constant("Rough", 0.7), constant("Zero", 0.0)],
        &[("BaseColor", "Colour", 0, Some(RGB)), ("Roughness", "Rough", 0, None), ("Metallic", "Zero", 0, None)],
    );
    // Specular is not written, so it reads 0
    same(&reads, &direct, "virtual texture read back");
}

#[test]
fn tangent_z_is_the_vertex_normal_and_the_normal_is_tangent_z() {
    let transform = |name: &str, from: &str, to: &str, input: &str| {
        json!({
            "name": name, "class": "MaterialExpressionTransform",
            "properties": { "TransformSourceType": format!("TRANSFORMSOURCE_{from}"), "TransformType": format!("TRANSFORM_{to}") },
            "inputs": [{ "index": 0, "name": "Input", "from": [input, 0], "mask": null }], "outputs": outputs(1)
        })
    };
    let normal = json!({ "name": "Normal", "class": "MaterialExpressionVertexNormalWS", "properties": {}, "inputs": [], "outputs": outputs(1) });
    let to_world = document(
        vec![constant3("Up", [0.0, 0.0, 1.0]), transform("ToWorld", "Tangent", "World", "Up")],
        &[("EmissiveColor", "ToWorld", 0, None)],
    );
    let vertex = document(vec![normal.clone()], &[("EmissiveColor", "Normal", 0, None)]);
    same(&to_world, &vertex, "tangent (0, 0, 1) in world space");

    let to_tangent = document(
        vec![normal, transform("ToTangent", "World", "Tangent", "Normal")],
        &[("Roughness", "ToTangent", 0, Some([0, 0, 1, 0]))],
    );
    let one = document(vec![constant("One", 1.0)], &[("Roughness", "One", 0, None)]);
    same(&to_tangent, &one, "the vertex normal's z in tangent space");
}
