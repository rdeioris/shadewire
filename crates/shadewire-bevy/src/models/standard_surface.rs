//! `standard_surface` onto Bevy's PBR input.
//!
//! | Bevy | standard_surface |
//! |---|---|
//! | `material.base_color.rgb` | `base * base_color` |
//! | `material.base_color.a` | luminance of `opacity` (Rec. 709 weights, the working space) |
//! | `material.metallic` | `metalness` |
//! | `material.perceptual_roughness` | `specular_roughness` |
//! | `material.emissive.rgb` | `emission * emission_color` |
//! | `N` | `normal` |
//! | `material.clearcoat`, `material.clearcoat_perceptual_roughness`, `clearcoat_N` | `coat`, `coat_roughness`, `coat_normal` |
//! | `material.specular_transmission`, `material.ior` | `transmission`, `specular_IOR` |
//!
//! Without a Bevy counterpart, ignored: `diffuse_roughness`, `specular`, `specular_color`,
//! `specular_anisotropy`, `specular_rotation`, `transmission_color`, `transmission_depth`,
//! `transmission_scatter`, `transmission_scatter_anisotropy`, `transmission_dispersion`,
//! `transmission_extra_roughness`, `subsurface*`, `sheen*`, `coat_color`, `coat_anisotropy`,
//! `coat_rotation`, `coat_IOR`, `coat_affect_color`, `coat_affect_roughness`, `thin_film_*`,
//! `thin_walled`, `tangent`.

pub const MAPPING: &str = "    pbr_input.material.base_color = vec4<f32>(s.base * s.base_color, dot(s.opacity, vec3<f32>(0.2126, 0.7152, 0.0722)));
    pbr_input.material.metallic = s.metalness;
    pbr_input.material.perceptual_roughness = s.specular_roughness;
    pbr_input.material.emissive = vec4<f32>(s.emission * s.emission_color, pbr_input.material.emissive.a);
    pbr_input.N = normalize(s.normal);
    pbr_input.material.clearcoat = s.coat;
    pbr_input.material.clearcoat_perceptual_roughness = s.coat_roughness;
    pbr_input.clearcoat_N = normalize(s.coat_normal);
    pbr_input.material.specular_transmission = s.transmission;
    pbr_input.material.ior = s.specular_IOR;
";
