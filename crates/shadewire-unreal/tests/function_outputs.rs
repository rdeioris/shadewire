//! A material function call's outputs read the function outputs they are named after, not the ones at the same
//! position by `SortPriority`. Unreal orders outputs of equal priority with an unstable sort, so its calls need not
//! follow the listing: MF_LandsapeLayers_01 gives each layer's Mat Attr and Heightmap outputs the same priority, and
//! its calls list Heightmap first.

use serde_json::{Value, json};
use shadewire::testing::assert_same_material;
use shadewire_unreal::{IrOptions, to_ir};

fn constant3(name: &str, value: [f32; 3]) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionConstant3Vector",
        "properties": { "Constant": [value[0], value[1], value[2], 1.0] },
        "inputs": [],
        "outputs": [
            { "name": "", "mask": [1, 1, 1, 0] }, { "name": "", "mask": [1, 0, 0, 0] },
            { "name": "", "mask": [0, 1, 0, 0] }, { "name": "", "mask": [0, 0, 1, 0] }
        ]
    })
}

fn function_output(name: &str, output_name: &str, from: &str) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionFunctionOutput",
        "properties": { "OutputName": output_name, "Description": "", "SortPriority": 0, "bLastPreviewed": false },
        "inputs": [{ "index": 0, "name": "", "from": [from, 0], "mask": null }],
        "outputs": [{ "name": "", "mask": null }]
    })
}

/// A material whose base colour is `expressions`' `base` output `out`, with `functions` beside it.
fn document(expressions: Vec<Value>, base: &str, out: usize, functions: Vec<Value>) -> String {
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
            "pins": [{ "pin": "BaseColor", "from": [base, out], "mask": null }],
            "expressions": expressions
        }],
        "parents": [],
        "functions": functions,
        "textures": []
    })
    .to_string()
}

const COLOR: [f32; 3] = [0.8, 0.6, 0.4];
const HEIGHT: [f32; 3] = [0.25, 0.25, 0.25];

#[test]
fn a_call_output_reads_the_function_output_of_its_name() {
    // MF_ColorAndHeight lists Color before Heightmap, both at priority 0; the call, like Unreal's, has Heightmap at
    // index 0 and Color at 1, and the base colour reads index 1.
    let function = json!({
        "path": "/Game/Test/MF_ColorAndHeight.MF_ColorAndHeight",
        "name": "MF_ColorAndHeight",
        "class": "MaterialFunction",
        "expressions": [
            constant3("MaterialExpressionConstant3Vector_0", COLOR),
            constant3("MaterialExpressionConstant3Vector_1", HEIGHT),
            function_output("MaterialExpressionFunctionOutput_0", "Color", "MaterialExpressionConstant3Vector_0"),
            function_output("MaterialExpressionFunctionOutput_1", "Heightmap", "MaterialExpressionConstant3Vector_1")
        ]
    });
    let call = json!({
        "name": "MaterialExpressionMaterialFunctionCall_0",
        "class": "MaterialExpressionMaterialFunctionCall",
        "properties": {
            "MaterialFunction": "/Game/Test/MF_ColorAndHeight.MF_ColorAndHeight",
            "FunctionInputs": [],
            "FunctionOutputs": [
                { "Output": { "OutputName": "Heightmap", "Mask": 0, "MaskR": 0, "MaskG": 0, "MaskB": 0, "MaskA": 0 } },
                { "Output": { "OutputName": "Color", "Mask": 0, "MaskR": 0, "MaskG": 0, "MaskB": 0, "MaskA": 0 } }
            ]
        },
        "inputs": [],
        "outputs": [{ "name": "Heightmap", "mask": null }, { "name": "Color", "mask": null }]
    });
    let called = document(vec![call], "MaterialExpressionMaterialFunctionCall_0", 1, vec![function]);
    let direct = document(vec![constant3("MaterialExpressionConstant3Vector_0", COLOR)], "MaterialExpressionConstant3Vector_0", 0, vec![]);

    let options = IrOptions::default();
    let called = to_ir(&called, None, &options).unwrap();
    let direct = to_ir(&direct, None, &options).unwrap();
    assert!(called.warnings.is_empty(), "{:?}", called.warnings);
    assert_same_material(&called.dag, &direct.dag, "Color through MF_ColorAndHeight");
}
