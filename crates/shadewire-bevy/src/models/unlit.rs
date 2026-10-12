//! `surface_unlit` onto Bevy's PBR input: the color goes out without lighting. The fragment entry
//! skips `apply_pbr_lighting` when `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` is set, as Bevy's own fragment
//! does (bevy_pbr 0.20.0 src/render/pbr.wesl:83-87); post-lighting processing (tonemapping, fog) still
//! runs.
//!
//! | Bevy | surface_unlit |
//! |---|---|
//! | `material.base_color.rgb` | `emission * emission_color / (1 - transmission)` (floored at 1e-4) |
//! | `material.base_color.a` | `opacity * (1 - transmission)` |
//!
//! `mx_surface_unlit` outputs color `emission * emission_color * opacity` over a transparency of
//! `1 - opacity * (1 - transmission)` (grey transmission color). Bevy's blend multiplies the color by
//! the alpha, so dividing by `1 - transmission` gives back exactly that color. With an opaque base
//! material the alpha is dropped and the color stays divided (brighter where transmission is set).
//! | `material.flags` | `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` set |
//!
//! Ignored: `transmission_color` (Bevy blends with the alpha alone).

pub const MAPPING: &str =
    "    pbr_input.material.base_color = vec4<f32>(s.emission * s.emission_color / max(1.0 - s.transmission, 1e-4), s.opacity * (1.0 - s.transmission));
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_is_divided_by_the_alpha_the_blend_multiplies_back() {
        assert!(MAPPING.contains(
            "vec4<f32>(s.emission * s.emission_color / max(1.0 - s.transmission, 1e-4), s.opacity * (1.0 - s.transmission))"
        ));
    }
}
