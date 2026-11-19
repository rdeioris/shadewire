//! A material function call's inputs reach the function inputs they are named after, not the ones at the same
//! position in the function's expression list. Unreal orders function inputs of equal `SortPriority` with an unstable
//! sort, so its calls need not follow the listing: the engine's Blend_Overlay lists Blend before Base, and its calls
//! take Base first.

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

fn subtract(name: &str, a: &str, b: &str) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionSubtract",
        "properties": { "ConstA": 1.0, "ConstB": 1.0 },
        "inputs": [
            { "index": 0, "name": "A", "from": [a, 0], "mask": null },
            { "index": 1, "name": "B", "from": [b, 0], "mask": null }
        ],
        "outputs": [{ "name": "", "mask": null }]
    })
}

fn function_input(name: &str, input_name: &str) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionFunctionInput",
        "properties": {
            "InputName": input_name, "Description": "", "InputType": "FunctionInput_Vector3",
            "PreviewValue": [0, 0, 0, 1], "bUsePreviewValueAsDefault": false, "SortPriority": 0,
            "BlendInputRelevance": "General"
        },
        "inputs": [{ "index": 0, "name": "Preview", "from": null, "mask": null }],
        "outputs": [{ "name": "", "mask": null }]
    })
}

/// A material whose base colour is `expressions`' `base` output, with `functions` beside it.
fn document(expressions: Vec<Value>, base: &str, functions: Vec<Value>) -> String {
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
            "pins": [{ "pin": "BaseColor", "from": [base, 0], "mask": null }],
            "expressions": expressions
        }],
        "parents": [],
        "functions": functions,
        "textures": []
    })
    .to_string()
}

const BASE: [f32; 3] = [0.8, 0.6, 0.4];
const BLEND: [f32; 3] = [0.1, 0.2, 0.3];

#[test]
fn a_call_input_reaches_the_function_input_of_its_name() {
    // MF_BaseMinusBlend returns Base - Blend and lists Blend first, as Blend_Overlay does; the call, like Unreal's,
    // takes Base at index 0.
    let function = json!({
        "path": "/Game/Test/MF_BaseMinusBlend.MF_BaseMinusBlend",
        "name": "MF_BaseMinusBlend",
        "class": "MaterialFunction",
        "expressions": [
            function_input("MaterialExpressionFunctionInput_1", "Blend"),
            function_input("MaterialExpressionFunctionInput_0", "Base"),
            subtract("MaterialExpressionSubtract_0", "MaterialExpressionFunctionInput_0", "MaterialExpressionFunctionInput_1"),
            {
                "name": "MaterialExpressionFunctionOutput_0",
                "class": "MaterialExpressionFunctionOutput",
                "properties": { "OutputName": "Result", "Description": "", "SortPriority": 0, "bLastPreviewed": false },
                "inputs": [{ "index": 0, "name": "", "from": ["MaterialExpressionSubtract_0", 0], "mask": null }],
                "outputs": [{ "name": "", "mask": null }]
            }
        ]
    });
    let call = json!({
        "name": "MaterialExpressionMaterialFunctionCall_0",
        "class": "MaterialExpressionMaterialFunctionCall",
        "properties": {
            "MaterialFunction": "/Game/Test/MF_BaseMinusBlend.MF_BaseMinusBlend",
            "FunctionInputs": [{}, {}],
            "FunctionOutputs": [{ "Output": { "OutputName": "Result", "Mask": 0, "MaskR": 0, "MaskG": 0, "MaskB": 0, "MaskA": 0 } }]
        },
        "inputs": [
            { "index": 0, "name": "Base (V3)", "from": ["MaterialExpressionConstant3Vector_0", 0], "mask": [1, 1, 1, 0] },
            { "index": 1, "name": "Blend (V3)", "from": ["MaterialExpressionConstant3Vector_1", 0], "mask": [1, 1, 1, 0] }
        ],
        "outputs": [{ "name": "Result", "mask": null }]
    });
    let called = document(
        vec![
            call,
            constant3("MaterialExpressionConstant3Vector_0", BASE),
            constant3("MaterialExpressionConstant3Vector_1", BLEND),
        ],
        "MaterialExpressionMaterialFunctionCall_0",
        vec![function],
    );
    // The same Base - Blend, with no function in between.
    let direct = document(
        vec![
            constant3("MaterialExpressionConstant3Vector_0", BASE),
            constant3("MaterialExpressionConstant3Vector_1", BLEND),
            subtract(
                "MaterialExpressionSubtract_0",
                "MaterialExpressionConstant3Vector_0",
                "MaterialExpressionConstant3Vector_1",
            ),
        ],
        "MaterialExpressionSubtract_0",
        vec![],
    );

    let options = IrOptions::default();
    let called = to_ir(&called, None, &options).unwrap();
    let direct = to_ir(&direct, None, &options).unwrap();
    assert!(called.warnings.is_empty(), "{:?}", called.warnings);
    assert_same_material(&called.dag, &direct.dag, "Base - Blend through MF_BaseMinusBlend");
}
