//! `surface_unlit` onto Bevy's PBR input: the color goes out without lighting. The fragment entry
//! skips `apply_pbr_lighting` when `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` is set, as Bevy's own fragment
//! does (bevy_pbr 0.20.0 src/render/pbr.wesl:83-87); post-lighting processing (tonemapping, fog) still
//! runs.
//!
//! | Bevy | surface_unlit |
//! |---|---|
//! | `material.base_color.rgb` | `E = emission * emission_color`; under `AlphaMode::Blend` divided by `1 - transmission` (floored at 1e-4), under `AlphaMode::Premultiplied` multiplied by `opacity` |
//! | `material.base_color.a` | `opacity * (1 - transmission)` |
//! | `material.flags` | `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` set |
//!
//! `mx_surface_unlit` outputs color `E * opacity` over a transparency of `1 - opacity * (1 - transmission)`
//! (grey transmission color), so the exact result is `E * opacity + (1 - alpha) * background` with
//! `alpha = opacity * (1 - transmission)`. The base material's alpha mode is read from its flags
//! (`STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS`, bevy_pbr 0.20.0 src/render/pbr_types.wesl:56-61):
//!
//! - `Blend` (`BlendState::ALPHA_BLENDING`, `color * alpha + (1 - alpha) * background`): the color divided
//!   by `1 - transmission` gives back exactly the MaterialX result.
//! - `Premultiplied` (`BlendState::PREMULTIPLIED_ALPHA_BLENDING`, `color + (1 - alpha) * background`;
//!   `premultiply_alpha` leaves the color alone, pbr_functions.wesl:949-951): the color times `opacity` is
//!   exactly the MaterialX result.
//! - `Add` (`premultiply_alpha` writes `color * alpha` with alpha 0, pbr_functions.wesl:936-948): the
//!   color undivided gives `E * opacity * (1 - transmission) + background`. Additive blending never
//!   attenuates the background, and the added light is MaterialX's only without transmission.
//! - Opaque, mask and the others: the color undivided.
//!
//! Ignored: `transmission_color` (Bevy blends with the alpha alone).

pub const MAPPING: &str = "    var shadewire_unlit_color = s.emission * s.emission_color;
    let shadewire_alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if shadewire_alpha_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND {
        shadewire_unlit_color /= max(1.0 - s.transmission, 1e-4);
    } else if shadewire_alpha_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED {
        shadewire_unlit_color *= s.opacity;
    }
    pbr_input.material.base_color = vec4<f32>(shadewire_unlit_color, s.opacity * (1.0 - s.transmission));
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_is_divided_by_the_alpha_under_blend() {
        assert!(MAPPING.starts_with("    var shadewire_unlit_color = s.emission * s.emission_color;
"));
        assert!(MAPPING.contains(
            "    if shadewire_alpha_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND {
        shadewire_unlit_color /= max(1.0 - s.transmission, 1e-4);
    }"
        ));
        assert!(MAPPING.contains(
            "vec4<f32>(shadewire_unlit_color, s.opacity * (1.0 - s.transmission))"
        ));
    }

    /// Premultiplied blending adds the color as is: it must already be MaterialX's `E * opacity`.
    #[test]
    fn color_is_premultiplied_by_the_opacity_under_premultiplied() {
        assert!(MAPPING.contains(
            " else if shadewire_alpha_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_PREMULTIPLIED {
        shadewire_unlit_color *= s.opacity;
    }"
        ));
    }
}
