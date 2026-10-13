//! Godot names: the uniforms' (named globals, which Godot binds by name) and the locals of the generated
//! `fragment()` and `light()`.

use shadewire::Reflection;
use shadewire::shader::names::{ident, is_reserved, struct_name};

use crate::context::{CONTEXT, CONTEXT_STRUCT};
use crate::reserved::RESERVED;

/// The local of `fragment()` and `light()` that holds the surface.
pub const SURFACE: &str = "s";

/// A node value's name: `n` and digits.
fn is_node_name(name: &str) -> bool {
    name.strip_prefix('n').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// One unique Godot uniform name per reflection uniform, index-aligned with `reflection.uniforms`: the authored
/// name made an identifier, with a trailing `_` when it would meet a Godot keyword or built-in, a name the
/// module declares (Godot refuses a parameter or local named like a global), a node value, the context or
/// surface local, or a `shadewire_` name; `_2`, `_3`... when two map alike.
pub fn uniform_names(reflection: &Reflection, declared: &[String]) -> Vec<String> {
    let model = struct_name(&reflection.model);
    let taken = |name: &str| {
        RESERVED.contains(&name)
            || is_reserved(name)
            || declared.iter().any(|d| d == name)
            || is_node_name(name)
            || [CONTEXT, SURFACE, CONTEXT_STRUCT, model.as_str()].contains(&name)
            || name.starts_with("shadewire_")
    };
    let mut names: Vec<String> = Vec::new();
    for u in &reflection.uniforms {
        let mut base = ident(&u.name);
        if taken(&base) {
            base.push('_');
        }
        let mut candidate = base.clone();
        let mut n = 2;
        while names.contains(&candidate) {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        names.push(candidate);
    }
    names
}

/// A surface field's member name in the model struct.
pub fn member(field: &str) -> String {
    ident(field)
}
