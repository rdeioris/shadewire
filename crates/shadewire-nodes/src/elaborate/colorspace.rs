//! Colorspace conversion through the cmlib nodedefs
//! `ND_<from>_to_<working>_<type>`, inlined like any implementation graph.

use std::collections::HashMap;

use shadewire::{Error, Location, NodeId, Type};

use super::{Binding, Elaborator};

/// Names that mean "not color, no conversion":
/// - empty, and MaterialX's `none` (MaterialX 1.39 specification, "Color Spaces and Color Management Systems");
/// - `data`, plus `raw` and `identity` kept "for compatibility with existing production assets"
///   (OpenUSD 25.08 `pxr/base/gf/colorSpace.h`, `GfColorSpaceNames`);
/// - `Raw` and its alias `Utility - Raw`, `isdata: true` in the OCIO studio config
///   (`studio-config-v2.2.0_aces-v1.3_ocio-v2.4`, built into OpenColorIO 2.4).
fn is_no_colorspace(name: &str) -> bool {
    matches!(name, "" | "none" | "raw" | "data" | "identity" | "Raw" | "Utility - Raw")
}

/// USD and OpenColorIO names for the colorspaces cmlib implements, with the cmlib name each stands for.
/// Matched exactly, without case folding: every name below is spelled as its source spells it. Sources:
/// - MX: MaterialX v1.39.5 `source/MaterialXGenShader/DefaultColorManagementSystem.cpp`, `COLOR_SPACE_REMAP`
///   (legacy names and the ASWF ColorInterop `*_scene` names), and the 1.39 specification, which lists
///   `lin_ap1 (alias for "acescg")`;
/// - USD: OpenUSD 25.08 `pxr/base/gf/colorSpace.h`, `GfColorSpaceNames` (only names with a cmlib equivalent:
///   `lin_ap0_scene`, `lin_rec2020_scene`, `lin_ciexyzd65_scene` and `srgb_ap1_scene` have none);
/// - OCIO: OpenColorIO 2.4 built-in `studio-config-v2.2.0_aces-v1.3_ocio-v2.4`: each entry is the name or an
///   alias of the colorspace that also carries the cmlib name as an alias.
pub(super) const ALIASES: &[(&str, &str)] = &[
    // OCIO "Linear Rec.709 (sRGB)"; USD LinearRec709 / MX `lin_rec709_scene`.
    ("lin_rec709_scene", "lin_rec709"),
    ("lin_srgb", "lin_rec709"),
    ("lin_rec709_srgb", "lin_rec709"),
    ("Linear Rec.709 (sRGB)", "lin_rec709"),
    ("Utility - Linear - Rec.709", "lin_rec709"),
    ("Utility - Linear - sRGB", "lin_rec709"),
    // OCIO "sRGB Encoded Rec.709 (sRGB)"; USD SRGBRec709 / MX `srgb_rec709_scene`.
    ("srgb_rec709_scene", "srgb_texture"),
    ("srgb_tx", "srgb_texture"),
    ("srgb_encoded_rec709_srgb", "srgb_texture"),
    ("sRGB Encoded Rec.709 (sRGB)", "srgb_texture"),
    ("sRGB - Texture", "srgb_texture"),
    ("Utility - sRGB - Texture", "srgb_texture"),
    ("Input - Generic - sRGB - Texture", "srgb_texture"),
    // OCIO "Gamma 1.8 Encoded Rec.709"; USD G18Rec709 / MX `g18_rec709_scene`; MX legacy `gamma18`.
    ("g18_rec709_scene", "g18_rec709"),
    ("gamma18", "g18_rec709"),
    ("g18_rec709_tx", "g18_rec709"),
    ("g18_encoded_rec709", "g18_rec709"),
    ("Gamma 1.8 Encoded Rec.709", "g18_rec709"),
    ("Gamma 1.8 Rec.709 - Texture", "g18_rec709"),
    ("Utility - Gamma 1.8 - Rec.709 - Texture", "g18_rec709"),
    // OCIO "Gamma 2.2 Encoded Rec.709"; USD G22Rec709 / MX `g22_rec709_scene`; MX legacy `gamma22`.
    ("g22_rec709_scene", "g22_rec709"),
    ("gamma22", "g22_rec709"),
    ("g22_rec709_tx", "g22_rec709"),
    ("g22_encoded_rec709", "g22_rec709"),
    ("Gamma 2.2 Encoded Rec.709", "g22_rec709"),
    ("Gamma 2.2 Rec.709 - Texture", "g22_rec709"),
    ("Utility - Gamma 2.2 - Rec.709 - Texture", "g22_rec709"),
    // OCIO "Gamma 2.4 Encoded Rec.709"; MX legacy `gamma24`.
    ("gamma24", "rec709_display"),
    ("g24_rec709", "rec709_display"),
    ("g24_rec709_tx", "rec709_display"),
    ("g24_encoded_rec709", "rec709_display"),
    ("Gamma 2.4 Encoded Rec.709", "rec709_display"),
    ("Gamma 2.4 Rec.709 - Texture", "rec709_display"),
    ("Utility - Rec.709 - Display", "rec709_display"),
    // OCIO "ACEScg"; USD LinearAP1 / MX `lin_ap1_scene`; MX spec `lin_ap1`.
    ("lin_ap1_scene", "acescg"),
    ("lin_ap1", "acescg"),
    ("ACEScg", "acescg"),
    ("ACES - ACEScg", "acescg"),
    // OCIO "Gamma 2.2 Encoded AP1"; USD G22AP1 / MX `g22_ap1_scene`.
    ("g22_ap1_scene", "g22_ap1"),
    ("g22_ap1_tx", "g22_ap1"),
    ("g22_encoded_ap1", "g22_ap1"),
    ("Gamma 2.2 Encoded AP1", "g22_ap1"),
    ("Gamma 2.2 AP1 - Texture", "g22_ap1"),
    // OCIO "Linear AdobeRGB"; USD LinearAdobeRGB / MX `lin_adobergb_scene`.
    ("lin_adobergb_scene", "lin_adobergb"),
    ("Linear AdobeRGB", "lin_adobergb"),
    ("Utility - Linear - Adobe RGB", "lin_adobergb"),
    // OCIO "Gamma 2.2 Encoded AdobeRGB"; USD G22AdobeRGB / MX `g22_adobergb_scene`.
    ("g22_adobergb_scene", "adobergb"),
    ("g22_encoded_adobergb", "adobergb"),
    ("Gamma 2.2 Encoded AdobeRGB", "adobergb"),
    // OCIO "sRGB Encoded P3-D65"; USD SRGBP3D65 / MX `srgb_p3d65_scene`.
    ("srgb_p3d65_scene", "srgb_displayp3"),
    ("srgb_p3d65", "srgb_displayp3"),
    ("srgb_encoded_p3d65", "srgb_displayp3"),
    ("srgb_encoded_p3d65_tx", "srgb_displayp3"),
    ("sRGB Encoded P3-D65", "srgb_displayp3"),
    ("sRGB Encoded P3-D65 - Texture", "srgb_displayp3"),
    // OCIO "Linear P3-D65"; USD LinearP3D65 / MX `lin_p3d65_scene`.
    ("lin_p3d65_scene", "lin_displayp3"),
    ("lin_p3d65", "lin_displayp3"),
    ("Linear P3-D65", "lin_displayp3"),
    ("Linear Display P3", "lin_displayp3"),
    ("Utility - Linear - P3-D65", "lin_displayp3"),
];

/// The cmlib name of colorspace `name`: its entry in `ALIASES`, or `name` itself.
fn canonical_colorspace(name: &str) -> &str {
    ALIASES.iter().find(|(alias, _)| *alias == name).map_or(name, |(_, canonical)| canonical)
}

impl<'a> Elaborator<'a> {
    /// Converts a `color3`/`color4` value from colorspace `from` to the working
    /// colorspace. No colorspace, `none`, the working colorspace itself and
    /// other types leave the value unchanged.
    pub fn convert_colorspace(
        &mut self,
        value: NodeId,
        ty: Type,
        from: Option<&str>,
        at: &Location,
    ) -> Result<NodeId, Error> {
        let working = canonical_colorspace(&self.options.working_colorspace);
        let authored = match from {
            Some(name) if !is_no_colorspace(name) && canonical_colorspace(name) != working => name,
            _ => return Ok(value),
        };
        if !matches!(ty, Type::Color3 | Type::Color4) {
            return Ok(value);
        }
        let name = format!("ND_{}_to_{working}_{}", canonical_colorspace(authored), ty.mtlx_name());
        let unsupported = || Error::UnsupportedColorspace { at: at.clone(), colorspace: authored.to_string() };
        let nd = self.index.nodedef(&name).ok_or_else(unsupported)?;
        let graph = self.index.graph_for_nodedef(&name).ok_or_else(unsupported)?;
        let mut interface = HashMap::new();
        interface.insert("in".to_string(), Binding::Node(value));
        let path = format!("{}/{name}", at.path);
        self.inline_bound(nd, graph, interface, path, &name, None, at)
    }
}
