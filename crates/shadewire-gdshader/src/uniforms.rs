use shadewire::{Reflection, Type};

use crate::syntax::texture_uniform;
use crate::types::{gd_type, is_constant_literal, literal};

/// Colors keep the color picker without Godot's sRGB-to-linear conversion on upload: authored values are already
/// in the working colorspace.
fn hints(ty: Type) -> &'static str {
    match ty {
        Type::Color3 | Type::Color4 => " : source_color, color_conversion_disabled",
        _ => "",
    }
}

/// The live uniforms and textures in one inspector group; empty when there are none. Godot binds by name:
/// uniforms take `names`, textures `shadewire_t<index>`.
pub fn uniform_block(reflection: &Reflection, names: &[String]) -> String {
    if reflection.uniforms.is_empty() && reflection.textures.is_empty() {
        return String::new();
    }
    let mut out = String::from("group_uniforms material;\n");
    for (u, name) in reflection.uniforms.iter().zip(names) {
        // A default must be a constant expression; inf and nan are not.
        let default =
            if is_constant_literal(&u.default) { format!(" = {}", literal(&u.default)) } else { String::new() };
        out.push_str(&format!("uniform {} {name}{}{default};\n", gd_type(u.ty), hints(u.ty)));
    }
    for t in &reflection.textures {
        out.push_str(&texture_uniform(t));
    }
    out.push_str("group_uniforms;\n");
    out
}
