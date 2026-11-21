//! Materials that use material attributes reach their pins through static switches, SetMaterialAttributes and
//! BlendMaterialAttributes, not only through a MakeMaterialAttributes: each case is the same material as one that
//! links the pins directly. Unreal's rock masters (M_Master_Rock) switch between a blend toward a runtime virtual
//! texture and their own attributes, and were read as Unreal's default surface.

use serde_json::{Value, json};
use shadewire::testing::assert_same_material;
use shadewire_unreal::{IrOptions, to_ir};

const C1: [f32; 3] = [0.8, 0.6, 0.4];
const C2: [f32; 3] = [0.1, 0.2, 0.3];

fn outputs(n: usize) -> Value {
    Value::Array((0..n).map(|_| json!({ "name": "", "mask": null })).collect())
}

fn constant(name: &str, value: f32) -> Value {
    json!({
        "name": name, "class": "MaterialExpressionConstant", "properties": { "R": value },
        "inputs": [], "outputs": outputs(1)
    })
}

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

fn input(index: usize, name: &str, from: Option<&str>) -> Value {
    json!({ "index": index, "name": name, "from": from.map(|f| json!([f, 0])), "mask": null })
}

fn lerp(name: &str, a: &str, b: &str, alpha: &str) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionLinearInterpolate",
        "properties": { "ConstA": 0, "ConstB": 1, "ConstAlpha": 0.5 },
        "inputs": [input(0, "A", Some(a)), input(1, "B", Some(b)), input(2, "Alpha", Some(alpha))],
        "outputs": outputs(1)
    })
}

/// MakeMaterialAttributes with `pins` (input name, expression) linked.
fn make(name: &str, pins: &[(&str, &str)]) -> Value {
    let inputs: Vec<Value> = pins.iter().enumerate().map(|(i, (pin, from))| input(i, pin, Some(from))).collect();
    json!({
        "name": name, "class": "MaterialExpressionMakeMaterialAttributes", "properties": {},
        "inputs": inputs, "outputs": outputs(1)
    })
}

/// SetMaterialAttributes over `base` (or nothing), setting `pins` by their display names.
fn set(name: &str, base: Option<&str>, pins: &[(&str, &str)]) -> Value {
    let mut inputs = vec![input(0, "MaterialAttributes", base)];
    inputs.extend(pins.iter().enumerate().map(|(i, (pin, from))| input(i + 1, pin, Some(from))));
    json!({
        "name": name, "class": "MaterialExpressionSetMaterialAttributes", "properties": {},
        "inputs": inputs, "outputs": outputs(1)
    })
}

fn blend(name: &str, a: &str, b: &str, alpha: &str) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionBlendMaterialAttributes",
        "properties": { "PixelAttributeBlendType": "Blend", "VertexAttributeBlendType": "Blend" },
        "inputs": [input(0, "A", Some(a)), input(1, "B", Some(b)), input(2, "Alpha", Some(alpha))],
        "outputs": outputs(1)
    })
}

fn switch(name: &str, default: bool, when_true: &str, when_false: &str) -> Value {
    json!({
        "name": name,
        "class": "MaterialExpressionStaticSwitchParameter",
        "properties": { "DefaultValue": default, "DynamicBranch": false, "ParameterName": "Switch", "Group": "" },
        "inputs": [input(0, "True", Some(when_true)), input(1, "False", Some(when_false))],
        "outputs": outputs(1)
    })
}

/// A material reading `pins` (pin, expression), with material attributes when `attributes` is set (then `pins` is
/// the one MaterialAttributes pin).
fn document(attributes: bool, expressions: Vec<Value>, pins: &[(&str, &str)]) -> String {
    let pins: Vec<Value> =
        pins.iter().map(|(pin, from)| json!({ "pin": pin, "from": [from, 0], "mask": null })).collect();
    json!({
        "format": "shadewire-unreal-graph",
        "version": 1,
        "producer": "test",
        "substrate": false,
        "materials": [{
            "path": "/Game/Test/M_Test.M_Test", "name": "M_Test", "class": "Material", "parent": null,
            "settings": {
                "MaterialDomain": "MD_Surface", "BlendMode": "BLEND_Opaque", "ShadingModel": "MSM_DefaultLit",
                "TwoSided": false, "bTangentSpaceNormal": true, "bUseMaterialAttributes": attributes,
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

fn same(attributes: &str, direct: &str, what: &str) {
    let options = IrOptions::default();
    let attributes = to_ir(attributes, None, &options).unwrap();
    let direct = to_ir(direct, None, &options).unwrap();
    assert!(attributes.warnings.is_empty(), "{:?}", attributes.warnings);
    assert_same_material(&attributes.dag, &direct.dag, what);
}

#[test]
fn a_static_switch_picks_the_attributes_of_its_branch() {
    let attributes = document(
        true,
        vec![
            constant3("C1", C1),
            constant3("C2", C2),
            constant("R", 0.2),
            make("MakeTrue", &[("BaseColor", "C1")]),
            make("MakeFalse", &[("BaseColor", "C2"), ("Roughness", "R")]),
            switch("Switch", false, "MakeTrue", "MakeFalse"),
        ],
        &[("MaterialAttributes", "Switch")],
    );
    let direct =
        document(false, vec![constant3("C2", C2), constant("R", 0.2)], &[("BaseColor", "C2"), ("Roughness", "R")]);
    same(&attributes, &direct, "the False branch of a static switch");
}

#[test]
fn set_material_attributes_replaces_the_pins_it_sets() {
    let attributes = document(
        true,
        vec![
            constant3("C1", C1),
            constant3("C2", C2),
            constant("R", 0.2),
            make("Make", &[("BaseColor", "C1"), ("Roughness", "R")]),
            set("Set", Some("Make"), &[("Base Color", "C2")]),
        ],
        &[("MaterialAttributes", "Set")],
    );
    let direct =
        document(false, vec![constant3("C2", C2), constant("R", 0.2)], &[("BaseColor", "C2"), ("Roughness", "R")]);
    same(&attributes, &direct, "SetMaterialAttributes over a MakeMaterialAttributes");
}

#[test]
fn blend_material_attributes_lerps_every_pin_with_unreal_defaults_for_unset_ones() {
    // A sets base colour and roughness, B base colour and metallic: B's roughness is Unreal's 0.5 and A's metallic 0.
    let attributes = document(
        true,
        vec![
            constant3("C1", C1),
            constant3("C2", C2),
            constant("R", 0.2),
            constant("M", 0.7),
            constant("Alpha", 0.25),
            make("A", &[("BaseColor", "C1"), ("Roughness", "R")]),
            set("B", None, &[("Base Color", "C2"), ("Metallic", "M")]),
            blend("Blend", "A", "B", "Alpha"),
        ],
        &[("MaterialAttributes", "Blend")],
    );
    let direct = document(
        false,
        vec![
            constant3("C1", C1),
            constant3("C2", C2),
            constant("R", 0.2),
            constant("M", 0.7),
            constant("Half", 0.5),
            constant("Zero", 0.0),
            constant("Alpha", 0.25),
            lerp("BaseColor", "C1", "C2", "Alpha"),
            lerp("Roughness", "R", "Half", "Alpha"),
            lerp("Metallic", "Zero", "M", "Alpha"),
        ],
        &[("BaseColor", "BaseColor"), ("Roughness", "Roughness"), ("Metallic", "Metallic")],
    );
    same(&attributes, &direct, "BlendMaterialAttributes of two attribute sets");
}

#[test]
fn a_runtime_virtual_texture_reads_as_zero_at_the_width_of_each_output() {
    let sample = json!({
        "name": "Rvt",
        "class": "MaterialExpressionRuntimeVirtualTextureSample",
        "properties": { "VirtualTexture": "/Game/RVT/RVT_Environment.RVT_Environment", "MaterialType": "BaseColor_Normal_Specular" },
        "inputs": [input(0, "Coordinates", None), input(1, "World Position", None)],
        "outputs": [
            { "name": "BaseColor", "mask": [1, 1, 1, 0] }, { "name": "Specular", "mask": null },
            { "name": "Roughness", "mask": null }, { "name": "Normal", "mask": null },
            { "name": "WorldHeight", "mask": null }, { "name": "Mask", "mask": null },
            { "name": "Displacement", "mask": null }, { "name": "Mask4", "mask": null }
        ]
    });
    let attributes = document(
        true,
        vec![
            sample,
            json!({
                "name": "Set", "class": "MaterialExpressionSetMaterialAttributes", "properties": {},
                "inputs": [
                    input(0, "MaterialAttributes", None),
                    { "index": 1, "name": "Base Color", "from": ["Rvt", 0], "mask": [1, 1, 1, 0] },
                    { "index": 2, "name": "Roughness", "from": ["Rvt", 2], "mask": null }
                ],
                "outputs": outputs(1)
            }),
        ],
        &[("MaterialAttributes", "Set")],
    );
    let direct = document(
        false,
        vec![constant3("Black", [0.0; 3]), constant("Zero", 0.0)],
        &[("BaseColor", "Black"), ("Roughness", "Zero")],
    );
    let options = IrOptions::default();
    let attributes = to_ir(&attributes, None, &options).unwrap();
    let direct = to_ir(&direct, None, &options).unwrap();
    assert!(attributes.warnings.iter().any(|w| w.contains("runtime virtual texture")), "{:?}", attributes.warnings);
    assert_same_material(&attributes.dag, &direct.dag, "a runtime virtual texture's outputs");
}

fn break_attributes(name: &str, from: &str) -> Value {
    let names = ["BaseColor", "Metallic", "Specular", "Roughness"];
    let outputs: Vec<Value> = names.iter().map(|n| json!({ "name": n, "mask": null })).collect();
    json!({
        "name": name, "class": "MaterialExpressionBreakMaterialAttributes", "properties": {},
        "inputs": [input(0, "Attr", Some(from))], "outputs": outputs
    })
}

#[test]
fn break_material_attributes_reads_what_make_set_and_unreal_defaults_otherwise() {
    // BaseColor from Make; Specular unset, so Unreal's default 0.5 (through Roughness)
    let broken = document(
        false,
        vec![
            constant3("C1", C1),
            make("Make", &[("BaseColor", "C1")]),
            break_attributes("Break", "Make"),
            json!({ "name": "Spec", "class": "MaterialExpressionComponentMask", "properties": { "R": true },
                     "inputs": [{ "index": 0, "name": "Input", "from": ["Break", 2], "mask": null }], "outputs": outputs(1) }),
        ],
        &[("BaseColor", "Break"), ("Roughness", "Spec")],
    );
    let direct = document(
        false,
        vec![constant3("C1", C1), constant("Half", 0.5)],
        &[("BaseColor", "C1"), ("Roughness", "Half")],
    );
    same(&broken, &direct, "BreakMaterialAttributes");
}

#[test]
fn material_attributes_returned_by_a_function_are_read_through_the_call() {
    // MF_Layer: FunctionInput (the colour) -> MakeMaterialAttributes -> FunctionOutput; the material's
    // MaterialAttributes pin reads the call
    let function = json!({
        "path": "/Game/Test/MF_Layer.MF_Layer", "name": "MF_Layer",
        "expressions": [
            { "name": "Colour", "class": "MaterialExpressionFunctionInput",
              "properties": { "InputName": "Colour", "InputType": "FunctionInput_Vector3", "SortPriority": 0 },
              "inputs": [], "outputs": outputs(1) },
            make("Make", &[("BaseColor", "Colour")]),
            { "name": "Out", "class": "MaterialExpressionFunctionOutput",
              "properties": { "OutputName": "Result", "SortPriority": 0 },
              "inputs": [input(0, "None", Some("Make"))], "outputs": outputs(1) }
        ]
    });
    let call = json!({
        "name": "Call", "class": "MaterialExpressionMaterialFunctionCall",
        "properties": { "MaterialFunction": "/Game/Test/MF_Layer.MF_Layer" },
        "inputs": [input(0, "Colour", Some("C1"))], "outputs": outputs(1)
    });
    let mut through: Value =
        serde_json::from_str(&document(true, vec![constant3("C1", C1), call], &[("MaterialAttributes", "Call")]))
            .unwrap();
    through["functions"] = json!([function]);
    let direct = document(false, vec![constant3("C1", C1)], &[("BaseColor", "C1")]);
    same(&through.to_string(), &direct, "attributes through a function call");
}

#[test]
fn a_switch_takes_the_input_its_parameter_picks() {
    let switch = json!({
        "name": "Switch", "class": "MaterialExpressionSwitch",
        "properties": { "ConstSwitchValue": 0, "ConstDefault": 0, "Inputs": [{ "InputName": "First" }, { "InputName": "Second" }] },
        "inputs": [input(0, "SwitchValue", Some("Pick")), input(1, "Default", None), input(2, "First", Some("C1")), input(3, "Second", Some("C2"))],
        "outputs": outputs(1)
    });
    let pick = json!({ "name": "Pick", "class": "MaterialExpressionScalarParameter",
                       "properties": { "ParameterName": "Pick", "DefaultValue": 1.0 }, "inputs": [], "outputs": outputs(1) });
    let switched =
        document(false, vec![constant3("C1", C1), constant3("C2", C2), pick, switch], &[("BaseColor", "Switch")]);
    let direct = document(false, vec![constant3("C2", C2)], &[("BaseColor", "C2")]);
    let options = IrOptions::default();
    let switched = to_ir(&switched, None, &options).unwrap();
    let direct = to_ir(&direct, None, &options).unwrap();
    shadewire::testing::assert_same_evaluation(&switched.dag, &direct.dag, "Switch picks Second at 1");
}

#[test]
fn arithmetic_on_material_attributes_applies_to_every_attribute() {
    // Make(BaseColor = C1, Roughness = 0.8) * 0.5: base colour C1 / 2, roughness 0.4, specular Unreal's 0.5 / 2
    let multiply = json!({
        "name": "Half", "class": "MaterialExpressionMultiply", "properties": { "ConstA": 0.0, "ConstB": 0.5 },
        "inputs": [input(0, "A", Some("Make")), input(1, "B", None)], "outputs": outputs(1)
    });
    let scaled = document(
        true,
        vec![
            constant3("C1", C1),
            constant("Rough", 0.8),
            make("Make", &[("BaseColor", "C1"), ("Roughness", "Rough")]),
            multiply,
        ],
        &[("MaterialAttributes", "Half")],
    );
    let half = [C1[0] * 0.5, C1[1] * 0.5, C1[2] * 0.5];
    let direct = document(
        false,
        vec![constant3("Half", half), constant("Rough", 0.4), constant("Spec", 0.25), constant3("Black", [0.0; 3])],
        // the emissive default (black) times 0.5 is set too, black
        &[("BaseColor", "Half"), ("Roughness", "Rough"), ("Specular", "Spec"), ("EmissiveColor", "Black")],
    );
    same(&scaled, &direct, "MaterialAttributes * 0.5");
}

#[test]
fn a_rotator_at_angle_zero_is_its_coordinate() {
    let rotator = json!({
        "name": "Rotate", "class": "MaterialExpressionRotator",
        "properties": { "CenterX": 0.5, "CenterY": 0.5, "Speed": 0.25, "ConstCoordinate": 0 },
        "inputs": [input(0, "Coordinate", Some("UV")), input(1, "Time", Some("Zero"))], "outputs": outputs(1)
    });
    let uv = json!({ "name": "UV", "class": "MaterialExpressionTextureCoordinate", "properties": { "CoordinateIndex": 0 },
                     "inputs": [], "outputs": outputs(1) });
    let rotated = document(false, vec![uv.clone(), constant("Zero", 0.0), rotator], &[("BaseColor", "Rotate")]);
    let plain = document(false, vec![uv], &[("BaseColor", "UV")]);
    same(&rotated, &plain, "Rotator at angle 0");
}
