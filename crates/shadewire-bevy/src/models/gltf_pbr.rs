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
//! | `diffuse_occlusion` | multiplied by `occlusion` (keeps Bevy's own occlusion, such as SSAO) |
//! | `material.clearcoat`, `material.clearcoat_perceptual_roughness`, `clearcoat_N` | `clearcoat`, `clearcoat_roughness`, `clearcoat_normal` |
//! | `material.specular_transmission`, `material.ior`, `material.thickness` | `transmission`, `ior`, `thickness` |
//!
//! Ignored: `tangent`, `specular`, `specular_color`, `iridescence*`, `sheen_*`, `anisotropy_*`,
//! `dispersion`; `attenuation_distance` and `attenuation_color` (Bevy enables attenuation with a material
//! flag set on the CPU); `alpha_mode` and `alpha_cutoff`: the base `StandardMaterial`'s alpha mode picks
//! the pipeline's blending, so it stays the glTF's.

pub const MAPPING: &str = "    pbr_input.material.base_color = vec4<f32>(s.base_color, s.alpha);
    pbr_input.material.metallic = s.metallic;
    pbr_input.material.perceptual_roughness = s.roughness;
    pbr_input.material.emissive = vec4<f32>(s.emissive * s.emissive_strength, pbr_input.material.emissive.a);
    pbr_input.N = normalize(s.normal);
    pbr_input.diffuse_occlusion *= s.occlusion;
    pbr_input.material.clearcoat = s.clearcoat;
    pbr_input.material.clearcoat_perceptual_roughness = s.clearcoat_roughness;
    pbr_input.clearcoat_N = normalize(s.clearcoat_normal);
    pbr_input.material.specular_transmission = s.transmission;
    pbr_input.material.ior = s.ior;
    pbr_input.material.thickness = s.thickness;
";
