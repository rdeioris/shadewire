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
