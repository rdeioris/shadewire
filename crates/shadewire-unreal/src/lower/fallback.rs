//! Expressions with no meaning outside Unreal, or none here yet: a warning and a neutral value.

use shadewire::{Error, NodeId};

use super::{Lowerer, short};
use crate::document::Expression;

/// Expressions whose neutral value is one (depths and fades read as fully visible, far away).
const ONE: [&str; 6] =
    ["SceneDepth", "PixelDepth", "DepthFade", "DistanceToNearestSurface", "PixelDepthOffset", "AmbientOcclusion"];

pub fn neutral(l: &mut Lowerer, f: usize, x: &Expression) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let class = short(&x.class);
    if class == "Custom" {
        let width = match x.text("OutputType").unwrap_or("CMOT_Float1") {
            "CMOT_Float2" => 2,
            "CMOT_Float3" => 3,
            "CMOT_Float4" => 4,
            _ => 1,
        };
        l.warn(f, x, "Custom HLSL replaced by 0");
        return Ok(l.zero(width, &o));
    }
    let value = if ONE.contains(&class) { 1.0 } else { 0.0 };
    l.warn(f, x, format!("not supported, read as {value}"));
    Ok(l.k(value, &o))
}

/// The width of each output of a runtime virtual texture sample: BaseColor, Specular, Roughness, Normal,
/// WorldHeight, Mask, Displacement, Mask4.
const VIRTUAL_TEXTURE_OUTPUTS: [usize; 8] = [3, 1, 1, 3, 1, 1, 1, 4];

/// A runtime virtual texture is rendered by the scene that holds it (a landscape writing its material into it), so
/// there is none outside Unreal: every output reads 0 at its own width. Materials that blend toward the virtual
/// texture by height (`WorldPosition.z - WorldHeight`) then take their own side everywhere above the world origin.
pub fn virtual_texture(l: &mut Lowerer, f: usize, x: &Expression, out: usize) -> Result<NodeId, Error> {
    let o = l.origin(f, x);
    let texture = x.text("VirtualTexture").unwrap_or("");
    l.warn(f, x, format!("the runtime virtual texture {texture} only exists in Unreal's scene, its outputs read as 0"));
    let width = VIRTUAL_TEXTURE_OUTPUTS.get(out).copied().unwrap_or(1);
    Ok(l.zero(width, &o))
}
