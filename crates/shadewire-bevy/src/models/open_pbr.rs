//! `open_pbr_surface` onto Bevy's PBR input.
//!
//! | Bevy | open_pbr_surface |
//! |---|---|
//! | `material.base_color.rgb` | `base_weight * base_color` |
//! | `material.base_color.a` | `geometry_opacity` |
//! | `material.metallic` | `base_metalness` |
//! | `material.perceptual_roughness` | `specular_roughness` |
//! | `material.emissive.rgb` | `emission_luminance / 1000 * emission_color` (OpenPBR emission is in nits; 1000 nits is emissive 1.0 here, a chosen scale) |
//! | `N` | `geometry_normal` |
//! | `material.clearcoat`, `material.clearcoat_perceptual_roughness`, `clearcoat_N` | `coat_weight`, `coat_roughness`, `geometry_coat_normal` |
//! | `material.specular_transmission`, `material.ior` | `transmission_weight`, `specular_ior` |
//! | `material.flags` | `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` cleared (lit even over an unlit base) |
//!
//! Without a Bevy counterpart, ignored: `base_diffuse_roughness`, `specular_weight`, `specular_color`,
//! `specular_roughness_anisotropy`, `transmission_color`, `transmission_depth`, `transmission_scatter*`,
//! `transmission_dispersion_*`, `subsurface_*`, `fuzz_*`, `coat_color`, `coat_roughness_anisotropy`,
//! `coat_ior`, `coat_darkening`, `thin_film_*`, `geometry_thin_walled`, `geometry_tangent`,
//! `geometry_coat_tangent`.

pub const MAPPING: &str = "    pbr_input.material.base_color = vec4<f32>(s.base_weight * s.base_color, s.geometry_opacity);
    pbr_input.material.metallic = s.base_metalness;
    pbr_input.material.perceptual_roughness = s.specular_roughness;
    pbr_input.material.emissive = vec4<f32>(s.emission_luminance / 1000.0 * s.emission_color, pbr_input.material.emissive.a);
    pbr_input.N = normalize(s.geometry_normal);
    pbr_input.material.clearcoat = s.coat_weight;
    pbr_input.material.clearcoat_perceptual_roughness = s.coat_roughness;
    pbr_input.clearcoat_N = normalize(s.geometry_coat_normal);
    pbr_input.material.specular_transmission = s.transmission_weight;
    pbr_input.material.ior = s.specular_ior;
    pbr_input.material.flags &= ~STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
";
