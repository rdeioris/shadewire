//! `surface_unlit` onto Bevy's PBR input: the color goes out without lighting. The fragment entry
//! skips `apply_pbr_lighting` when `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` is set, as Bevy's own fragment
//! does (bevy_pbr 0.20.0 src/render/pbr.wesl:83-87); post-lighting processing (tonemapping, fog) still
//! runs.
//!
//! | Bevy | surface_unlit |
//! |---|---|
//! | `material.base_color.rgb` | `emission * emission_color`, divided by `1 - transmission` (floored at 1e-4) under `AlphaMode::Blend` |
//! | `material.base_color.a` | `opacity * (1 - transmission)` |
//! | `material.flags` | `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` set |
//!
//! `mx_surface_unlit` outputs color `emission * emission_color * opacity` over a transparency of
//! `1 - opacity * (1 - transmission)` (grey transmission color). Bevy's blend multiplies the color by
//! the alpha, so under `AlphaMode::Blend` dividing by `1 - transmission` gives back exactly that color.
//! The base material's alpha mode is read from its flags (`STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS`
//! equal to `STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND`, bevy_pbr 0.20.0 src/render/pbr_types.wesl:56, 59);
//! any other mode keeps the color undivided.
//!
//! Ignored: `transmission_color` (Bevy blends with the alpha alone).

pub const MAPPING: &str = "    var shadewire_unlit_divisor = 1.0;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS) == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND {
        shadewire_unlit_divisor = max(1.0 - s.transmission, 1e-4);
    }
    pbr_input.material.base_color = vec4<f32>(s.emission * s.emission_color / shadewire_unlit_divisor, s.opacity * (1.0 - s.transmission));
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_is_divided_by_the_alpha_only_under_blend() {
        assert!(MAPPING.contains(
            "if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS) == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND {\n        shadewire_unlit_divisor = max(1.0 - s.transmission, 1e-4);\n    }"
        ));
        assert!(MAPPING.starts_with("    var shadewire_unlit_divisor = 1.0;\n"));
        assert!(MAPPING.contains(
            "vec4<f32>(s.emission * s.emission_color / shadewire_unlit_divisor, s.opacity * (1.0 - s.transmission))"
        ));
    }
}
