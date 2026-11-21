//! Landscape layer nodes read each layer's paint weight from the geomprop `landscape_layer_<name>`; the blends are
//! Unreal's (weight blend, height blend renormalized, alpha blend last, material attributes relative to the defaults).
//! Each case is the same material as one built from the formula on the same geomprops.

use serde_json::{Value, json};
use shadewire::testing::assert_same_evaluation;
use shadewire_unreal::{IrOptions, to_ir};

fn outputs(n: usize) -> Value {
    Value::Array((0..n).map(|_| json!({ "name": "", "mask": null })).collect())
}

fn input(index: usize, name: &str, from: Option<&str>) -> Value {
    json!({ "index": index, "name": name, "from": from.map(|f| json!([f, 0])), "mask": null })
}

fn constant3(name: &str, value: [f32; 3]) -> Value {
    json!({ "name": name, "class": "MaterialExpressionConstant3Vector",
            "properties": { "Constant": [value[0], value[1], value[2], 1.0] }, "inputs": [], "outputs": outputs(1) })
}

fn sample(name: &str, layer: &str) -> Value {
    json!({ "name": name, "class": "MaterialExpressionLandscapeLayerSample",
            "properties": { "ParameterName": layer, "PreviewWeight": 1.0 }, "inputs": [], "outputs": outputs(1) })
}

fn op(name: &str, class: &str, a: &str, b: &str) -> Value {
    json!({ "name": name, "class": format!("MaterialExpression{class}"), "properties": {},
            "inputs": [input(0, "A", Some(a)), input(1, "B", Some(b))], "outputs": outputs(1) })
}

fn document(expressions: Vec<Value>, pins: &[(&str, &str)]) -> String {
    let pins: Vec<Value> =
        pins.iter().map(|(pin, from)| json!({ "pin": pin, "from": [from, 0], "mask": null })).collect();
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
    assert!(a.warnings.is_empty(), "{:?}", a.warnings);
    assert_same_evaluation(&a.dag, &b.dag, what);
}

fn blend(name: &str, layers: &[(&str, &str, &str)]) -> Value {
    let props: Vec<Value> = layers.iter().map(|(layer, blend, _)| json!({
        "LayerName": layer, "BlendType": blend, "PreviewWeight": 0, "ConstLayerInput": [0, 0, 0], "ConstHeightInput": 0
    })).collect();
    let inputs: Vec<Value> = layers
        .iter()
        .enumerate()
        .map(|(i, (layer, _, from))| input(i, &format!("Layer {layer}"), Some(from)))
        .collect();
    json!({ "name": name, "class": "MaterialExpressionLandscapeLayerBlend", "properties": { "Layers": props },
            "inputs": inputs, "outputs": outputs(1) })
}

#[test]
fn a_weight_blend_is_the_weighted_sum_of_its_layers() {
    let (c1, c2) = ([0.8, 0.6, 0.4], [0.1, 0.2, 0.3]);
    let blended = document(
        vec![
            constant3("Sand", c1),
            constant3("Grass", c2),
            blend("Blend", &[("Sand", "LB_WeightBlend", "Sand"), ("Grass Layer", "LB_WeightBlend", "Grass")]),
        ],
        &[("BaseColor", "Blend")],
    );
    // w_sand * sand + w_grass * grass, with the same geomprops (the layer name's space made `_`)
    let formula = document(
        vec![
            constant3("Sand", c1),
            constant3("Grass", c2),
            sample("WSand", "Sand"),
            sample("WGrass", "Grass Layer"),
            op("A", "Multiply", "Sand", "WSand"),
            op("B", "Multiply", "Grass", "WGrass"),
            op("Sum", "Add", "A", "B"),
        ],
        &[("BaseColor", "Sum")],
    );
    same(&blended, &formula, "weight blend");
}

#[test]
fn an_alpha_blend_lerps_over_the_weighted_layers() {
    let (c1, c2) = ([0.8, 0.6, 0.4], [0.1, 0.2, 0.3]);
    let blended = document(
        vec![
            constant3("Sand", c1),
            constant3("Snow", c2),
            blend("Blend", &[("Snow", "LB_AlphaBlend", "Snow"), ("Sand", "LB_WeightBlend", "Sand")]),
        ],
        &[("BaseColor", "Blend")],
    );
    let lerp = json!({ "name": "Lerp", "class": "MaterialExpressionLinearInterpolate",
                       "properties": { "ConstA": 0, "ConstB": 1, "ConstAlpha": 0.5 },
                       "inputs": [input(0, "A", Some("A")), input(1, "B", Some("Snow")), input(2, "Alpha", Some("WSnow"))], "outputs": outputs(1) });
    let formula = document(
        vec![
            constant3("Sand", c1),
            constant3("Snow", c2),
            sample("WSand", "Sand"),
            sample("WSnow", "Snow"),
            op("A", "Multiply", "Sand", "WSand"),
            lerp,
        ],
        &[("BaseColor", "Lerp")],
    );
    same(&blended, &formula, "alpha blend");
}

#[test]
fn layer_coords_scale_the_landscape_coordinates() {
    let coords = json!({ "name": "Coords", "class": "MaterialExpressionLandscapeLayerCoords",
                         "properties": { "MappingType": "TCMT_Auto", "CustomUVType": "LCCT_None", "MappingScale": 4.0,
                                         "MappingRotation": 0, "MappingPanU": 0.5, "MappingPanV": 0 },
                         "inputs": [], "outputs": outputs(1) });
    let uv = json!({ "name": "UV", "class": "MaterialExpressionTextureCoordinate",
                     "properties": { "CoordinateIndex": 0, "UTiling": 0.25, "VTiling": 0.25 }, "inputs": [], "outputs": outputs(1) });
    let pan = json!({ "name": "Pan", "class": "MaterialExpressionConstant2Vector", "properties": { "R": 0.5, "G": 0.0 }, "inputs": [], "outputs": outputs(1) });
    // through Base Color, which both read as the vector2 padded with 0
    let by_coords = document(vec![coords], &[("BaseColor", "Coords")]);
    let by_uv = document(vec![uv, pan, op("Sum", "Add", "UV", "Pan")], &[("BaseColor", "Sum")]);
    same(&by_coords, &by_uv, "layer coords: landscape coordinates / 4 + (0.5, 0)");
}
