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
