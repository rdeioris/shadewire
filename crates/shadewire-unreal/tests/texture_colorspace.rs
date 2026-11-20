//! Unreal stores an sRGB texture in an sRGB GPU format, so sampling decodes it whatever the sampler type: a material
//! instance can plug an sRGB texture into a Masks sampler (M_Master_Foliage's Mask, given T_BuffelGrass_Mask), and
//! the mask reads decoded. The texture's own sRGB flag decides its colorspace, not the sampler type.

use serde_json::json;
use shadewire_unreal::{IrOptions, to_ir};

fn colorspace(srgb: bool, sampler: &str) -> Option<String> {
    let texture = "/Game/Test/T_Mask.T_Mask";
    let document = json!({
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
            "pins": [{ "pin": "BaseColor", "from": ["Sample", 0], "mask": [1, 1, 1, 0] }],
            "expressions": [{
                "name": "Sample", "class": "MaterialExpressionTextureSample",
                "properties": { "Texture": texture, "SamplerType": sampler },
                "inputs": [], "outputs": [{ "name": "RGB", "mask": [1, 1, 1, 0] }]
            }]
        }],
        "parents": [],
        "functions": [],
        "textures": [{ "path": texture, "name": "T_Mask", "class": "Texture2D", "file": "Textures/T_Mask.png", "srgb": srgb }]
    })
    .to_string();
    let ir = to_ir(&document, None, &IrOptions::default()).unwrap();
    assert_eq!(ir.dag.textures.len(), 1, "{:?}", ir.dag.textures);
    ir.dag.textures[0].colorspace.clone()
}

#[test]
fn an_srgb_texture_is_decoded_whatever_the_sampler_type() {
    for sampler in ["SAMPLERTYPE_Color", "SAMPLERTYPE_LinearColor", "SAMPLERTYPE_Masks", "SAMPLERTYPE_Grayscale"] {
        assert_eq!(colorspace(true, sampler).as_deref(), Some("srgb_texture"), "{sampler}");
    }
}

#[test]
fn a_linear_texture_is_not_decoded_even_by_a_color_sampler() {
    for sampler in ["SAMPLERTYPE_Color", "SAMPLERTYPE_Masks"] {
        assert_eq!(colorspace(false, sampler), None, "{sampler}");
    }
}
