//! A CollectionParameter reads its material parameter collection's default value: the same material as one reading
//! that value from a constant. A collection the document lacks, or a parameter it lacks, reads 0 with a warning.

use serde_json::{Value, json};
use shadewire::testing::assert_same_evaluation;
use shadewire_unreal::{IrOptions, to_ir};

const COLLECTION: &str = "/Game/Test/MPC_Season.MPC_Season";

fn outputs(n: usize) -> Value {
    Value::Array((0..n).map(|_| json!({ "name": "", "mask": null })).collect())
}

fn collection_parameter(name: &str, collection: &str, parameter: &str) -> Value {
    json!({
        "name": name, "class": "MaterialExpressionCollectionParameter",
        "properties": { "Collection": collection, "ParameterName": parameter },
        "inputs": [], "outputs": outputs(1)
    })
}

/// `pins` are (pin, expression, channel mask).
fn document(expressions: Vec<Value>, pins: &[(&str, &str, Option<[u8; 4]>)], collections: Value) -> String {
    let pins: Vec<Value> =
        pins.iter().map(|(pin, from, mask)| json!({ "pin": pin, "from": [from, 0], "mask": mask })).collect();
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
        "textures": [],
        "collections": collections
    })
    .to_string()
}

fn season() -> Value {
    json!([{
        "path": COLLECTION, "name": "MPC_Season", "class": "MaterialParameterCollection",
        "scalars": [{ "name": "Dry", "value": 0.65 }],
        "vectors": [{ "name": "Tint", "value": [0.25, 0.5, 0.75, 1.0] }]
    }])
}

#[test]
fn a_collection_parameter_reads_its_collections_default() {
    let rgb = Some([1, 1, 1, 0]);
    let collection = document(
        vec![collection_parameter("Dry", COLLECTION, "Dry"), collection_parameter("Tint", COLLECTION, "Tint")],
        &[("Roughness", "Dry", None), ("BaseColor", "Tint", rgb)],
        season(),
    );
    let constants = document(
        vec![
            json!({ "name": "Dry", "class": "MaterialExpressionConstant", "properties": { "R": 0.65 },
                    "inputs": [], "outputs": outputs(1) }),
            json!({ "name": "Tint", "class": "MaterialExpressionConstant3Vector",
                    "properties": { "Constant": [0.25, 0.5, 0.75, 1.0] }, "inputs": [], "outputs": outputs(1) }),
        ],
        &[("Roughness", "Dry", None), ("BaseColor", "Tint", rgb)],
        json!([]),
    );
    let options = IrOptions::default();
    let collection = to_ir(&collection, None, &options).unwrap();
    let constants = to_ir(&constants, None, &options).unwrap();
    assert!(collection.warnings.is_empty(), "{:?}", collection.warnings);
    assert_same_evaluation(&collection.dag, &constants.dag, "CollectionParameter");
}

#[test]
fn a_missing_collection_or_parameter_reads_zero_with_a_warning() {
    let missing = document(
        vec![
            collection_parameter("Gone", "/Game/Test/MPC_Gone.MPC_Gone", "Dry"),
            collection_parameter("Unknown", COLLECTION, "Wet"),
        ],
        &[("Roughness", "Gone", None), ("Metallic", "Unknown", None)],
        season(),
    );
    let zeros = document(
        vec![json!({ "name": "Zero", "class": "MaterialExpressionConstant", "properties": { "R": 0.0 },
                     "inputs": [], "outputs": outputs(1) })],
        &[("Roughness", "Zero", None), ("Metallic", "Zero", None)],
        json!([]),
    );
    let options = IrOptions::default();
    let missing = to_ir(&missing, None, &options).unwrap();
    let zeros = to_ir(&zeros, None, &options).unwrap();
    assert_same_evaluation(&missing.dag, &zeros.dag, "missing collection parameters");
    assert!(
        missing.warnings.iter().any(|w| w.contains("MPC_Gone") && w.contains("not in the document")),
        "{:?}",
        missing.warnings
    );
    assert!(missing.warnings.iter().any(|w| w.contains("no parameter \"Wet\"")), "{:?}", missing.warnings);
}
