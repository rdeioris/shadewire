//! `gltf_pbr` onto Bevy's PBR input. Bevy's `StandardMaterial` is the glTF model, so most fields map
//! one to one.
//!
//! | Bevy | gltf_pbr |
//! |---|---|
//! | `material.base_color` | `base_color`, `alpha` |
//! | `material.metallic` | `metallic` |
//! | `material.perceptual_roughness` | `roughness` |
//! | `material.emissive.rgb` | `emissive * emissive_strength` |
//! | `N` | `normal` |
//! | `material.clearcoat`, `material.clearcoat_perceptual_roughness`, `clearcoat_N` | `clearcoat`, `clearcoat_roughness`, `clearcoat_normal` |
//! | `material.specular_transmission`, `material.ior`, `material.thickness` | `transmission`, `ior`, `thickness` |
//! | `material.flags` | `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` cleared (lit even over an unlit base) |
//!
//! Ignored: `occlusion` (MaterialX's own gltf_pbr ignores it; the base `StandardMaterial` keeps the glTF's
//! occlusion map), `tangent`, `specular`, `specular_color`, `iridescence*`, `sheen_*`, `anisotropy_*`,
//! `dispersion`; `attenuation_distance` and `attenuation_color` (Bevy enables attenuation with a material
//! flag set on the CPU); `alpha_mode` and `alpha_cutoff`: the base `StandardMaterial`'s alpha mode picks
//! the pipeline's blending, so it stays the glTF's.

pub const MAPPING: &str = "    pbr_input.material.base_color = vec4<f32>(s.base_color, s.alpha);
    pbr_input.material.metallic = s.metallic;
    pbr_input.material.perceptual_roughness = s.roughness;
    pbr_input.material.emissive = vec4<f32>(s.emissive * s.emissive_strength, pbr_input.material.emissive.a);
    pbr_input.N = normalize(s.normal);
    pbr_input.material.clearcoat = s.clearcoat;
    pbr_input.material.clearcoat_perceptual_roughness = s.clearcoat_roughness;
    pbr_input.clearcoat_N = normalize(s.clearcoat_normal);
    pbr_input.material.specular_transmission = s.transmission;
    pbr_input.material.ior = s.ior;
    pbr_input.material.thickness = s.thickness;
    pbr_input.material.flags &= ~STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
";

#[cfg(test)]
mod tests {
    use super::MAPPING;

    /// MaterialX's own gltf_pbr ignores `occlusion`; the base `StandardMaterial` keeps the glTF's occlusion map.
    #[test]
    fn occlusion_is_ignored() {
        assert!(!MAPPING.contains("occlusion"), "{MAPPING}");
        assert!(MAPPING.contains("pbr_input.N = normalize(s.normal);"));
    }
}
