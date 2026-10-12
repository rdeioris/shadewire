//! `surface_unlit` onto Bevy's PBR input: the color goes out without lighting. The fragment entry
//! skips `apply_pbr_lighting` when `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` is set, as Bevy's own fragment
//! does (bevy_pbr 0.20.0 src/render/pbr.wesl:83-87); post-lighting processing (tonemapping, fog) still
//! runs.
//!
//! | Bevy | surface_unlit |
//! |---|---|
//! | `material.base_color.rgb` | `emission * emission_color` |
//! | `material.base_color.a` | `opacity * (1 - transmission)` |
//! | `material.flags` | `STANDARD_MATERIAL_FLAGS_UNLIT_BIT` set |
//!
//! Ignored: `transmission_color` (Bevy blends with the alpha alone).

pub const MAPPING: &str =
    "    pbr_input.material.base_color = vec4<f32>(s.emission * s.emission_color, s.opacity * (1.0 - s.transmission));
    pbr_input.material.flags |= STANDARD_MATERIAL_FLAGS_UNLIT_BIT;
";
