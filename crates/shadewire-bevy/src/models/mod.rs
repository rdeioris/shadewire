//! Each shading model's surface struct (`s`) written onto Bevy's `PbrInput` (`pbr_input`).
//!
//! Bevy 0.20 field names (bevy_pbr 0.20.0, src/render/pbr_types.wesl): `PbrInput.material` (:97),
//! `PbrInput.diffuse_occlusion` (:99), `PbrInput.world_normal` (:106), `PbrInput.N` (:108), `PbrInput.V`
//! (:111), `PbrInput.clearcoat_N` (:113); `StandardMaterial.base_color` (:4), `.emissive` (:5),
//! `.perceptual_roughness` (:9), `.metallic` (:10), `.specular_transmission` (:12), `.thickness` (:13),
//! `.ior` (:14), `.clearcoat` (:16), `.clearcoat_perceptual_roughness` (:17), `.flags` (:21),
//! `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` (:40). `pbr_input_from_standard_material` fills them from the base
//! material first (src/render/pbr_fragment.wesl:71), so fields a model leaves alone keep the glTF's values.
//!
//! The emissive alpha is kept: Bevy scales emissive light by `mix(1.0, view.exposure, emissive.a)`
//! (src/render/pbr_functions.wesl:832). Clearcoat and specular transmission are lit only when the
//! base `StandardMaterial` turns them on: Bevy compiles them in through the `STANDARD_MATERIAL_CLEARCOAT`
//! and `STANDARD_MATERIAL_SPECULAR_TRANSMISSION` shader defs it derives from the base material.

mod gltf_pbr;
mod open_pbr;
mod standard_surface;
mod unlit;

use crate::error::ShadewireError;

/// The WGSL statements that write the model struct `s` onto `pbr_input`.
pub fn mapping(model: &str) -> Result<&'static str, ShadewireError> {
    match model {
        "standard_surface" => Ok(standard_surface::MAPPING),
        "open_pbr_surface" => Ok(open_pbr::MAPPING),
        "gltf_pbr" => Ok(gltf_pbr::MAPPING),
        "surface_unlit" => Ok(unlit::MAPPING),
        other => Err(ShadewireError::UnsupportedModel(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_one_model_has_a_mapping() {
        for model in ["standard_surface", "open_pbr_surface", "gltf_pbr", "surface_unlit"] {
            assert!(mapping(model).unwrap().contains("pbr_input.material.base_color = "), "{model}");
        }
        let err = mapping("disney_principled").unwrap_err();
        assert_eq!(err.to_string(), "shading model `disney_principled` has no Bevy mapping");
    }
}
