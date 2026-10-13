use shadewire::shader::names::ident;
use shadewire::{Reflection, Type, UiHints, Value};

use crate::reserved::RESERVED;
use crate::syntax::texture_uniform;
use crate::types::{gd_type, is_constant_literal, literal};

/// Colors keep the color picker without Godot's sRGB-to-linear conversion on upload: authored values are already
/// in the working colorspace.
fn hints(ty: Type, ui: &UiHints) -> String {
    match ty {
        Type::Color3 | Type::Color4 => " : source_color, color_conversion_disabled".to_string(),
        Type::Float | Type::Integer => {
            range(ui).map(|(lo, hi)| format!(" : hint_range({lo}, {hi})")).unwrap_or_default()
        }
        _ => String::new(),
    }
}

/// The inspector range of a scalar: each end from the soft limit when authored, else the hard one. `None` unless
/// both ends are known and ordered.
fn range(ui: &UiHints) -> Option<(String, String)> {
    let lo = ui.soft_min.as_ref().or(ui.min.as_ref())?;
    let hi = ui.soft_max.as_ref().or(ui.max.as_ref())?;
    let number = |v: &Value| match v {
        Value::Float(x) if x.is_finite() => Some(*x as f64),
        // `hint_range` takes a literal; `i32::MIN` only prints as an expression.
        Value::Integer(i) if *i != i32::MIN => Some(*i as f64),
        _ => None,
    };
    (number(lo)? < number(hi)?).then(|| (literal(lo), literal(hi)))
}

/// The `group_uniforms` name of a `uifolder`: `/` separates a group from its subgroup, and Godot has one
/// level of subgroups, so deeper folders join into the subgroup with `_`. Each part is made an identifier.
fn group(folder: &str) -> Option<String> {
    let parts: Vec<String> = folder
        .split('/')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            let mut name = ident(p.trim());
            if RESERVED.contains(&name.as_str()) {
                name.push('_');
            }
            name
        })
        .collect();
    match parts.split_first()? {
        (first, []) => Some(first.clone()),
        (first, rest) => Some(format!("{first}.{}", rest.join("_"))),
    }
}

/// The live uniforms and textures by inspector group; empty when there are none. Uniforms without a `uifolder`
/// and every texture go in `material`, then each folder's group in the order folders first appear. Godot binds by
/// name: uniforms take `names`, textures `shadewire_t<index>`.
pub fn uniform_block(reflection: &Reflection, names: &[String]) -> String {
    if reflection.uniforms.is_empty() && reflection.textures.is_empty() {
        return String::new();
    }
    let mut material = String::new();
    let mut folders: Vec<(String, String)> = Vec::new();
    for (u, name) in reflection.uniforms.iter().zip(names) {
        // A default must be a constant expression; inf and nan are not.
        let default =
            if is_constant_literal(&u.default) { format!(" = {}", literal(&u.default)) } else { String::new() };
        let line = format!(
            "uniform {} {name}{}{default};
",
            gd_type(u.ty),
            hints(u.ty, &u.ui)
        );
        match u.ui.folder.as_deref().and_then(group) {
            Some(g) => match folders.iter_mut().find(|(name, _)| *name == g) {
                Some((_, body)) => body.push_str(&line),
                None => folders.push((g, line)),
            },
            None => material.push_str(&line),
        }
    }
    for t in &reflection.textures {
        material.push_str(&texture_uniform(t));
    }
    let mut out = String::new();
    if !material.is_empty() {
        out.push_str(
            "group_uniforms material;
",
        );
        out.push_str(&material);
    }
    for (g, body) in &folders {
        out.push_str(&format!(
            "group_uniforms {g};
"
        ));
        out.push_str(body);
    }
    out.push_str(
        "group_uniforms;
",
    );
    out
}
