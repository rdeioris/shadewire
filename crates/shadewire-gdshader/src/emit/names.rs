use shadewire::Reflection;

use super::reserved::RESERVED;
use crate::leaf_library;

/// The context struct, the parameter of the entry function that carries it, and the
/// local in `fragment()` that holds the surface.
pub const CONTEXT_STRUCT: &str = "ShadewireContext";
pub const CONTEXT: &str = "ctx";
pub const SURFACE: &str = "s";

/// Makes an authored name a valid Godot identifier: invalid characters become `_`,
/// a leading digit gets a `_` prefix, reserved words get a trailing `_`.
pub fn ident(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    if out == "_" || out.starts_with("__") {
        out.insert(0, 'x');
    }
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// A node value's name: `n` and digits.
fn is_node_name(name: &str) -> bool {
    name.strip_prefix('n').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether a global name would meet something the generated code declares: Godot refuses a
/// parameter or local named like a global, so uniforms keep clear of the leaf library's
/// identifiers, the node values, the context and surface locals and every `shadewire_` name.
fn taken(name: &str, model_struct: &str) -> bool {
    RESERVED.contains(&name)
        || leaf_library::identifiers().contains(name)
        || is_node_name(name)
        || [CONTEXT, SURFACE, CONTEXT_STRUCT, model_struct].contains(&name)
        || name.starts_with("shadewire_")
}

/// One unique Godot uniform name per reflection uniform, index-aligned with `reflection.uniforms`.
/// Godot binds by name, so hosts read these names from `GdshaderOutput::uniform_names`;
/// reflection keeps the authored ones.
pub struct UniformNames {
    names: Vec<String>,
}

impl UniformNames {
    pub fn new(reflection: &Reflection) -> UniformNames {
        let model_struct = struct_name(&reflection.model);
        let mut names: Vec<String> = Vec::new();
        for u in &reflection.uniforms {
            let mut base = ident(&u.name);
            if taken(&base, &model_struct) {
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
        UniformNames { names }
    }

    pub fn name(&self, index: usize) -> &str {
        &self.names[index]
    }

    pub fn into_vec(self) -> Vec<String> {
        self.names
    }
}

/// The struct name of a shading model.
pub fn struct_name(model: &str) -> String {
    shadewire::model_struct_name(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_words_get_an_underscore() {
        assert_eq!(ident("uniform"), "uniform_");
        assert_eq!(ident("source_color"), "source_color_");
        assert_eq!(ident("TIME"), "TIME_");
        assert_eq!(ident("base_color"), "base_color");
    }

    #[test]
    fn invalid_characters_are_replaced() {
        assert_eq!(ident("my-prop.x"), "my_prop_x");
        assert_eq!(ident("2d"), "_2d");
        assert_eq!(ident("_"), "x_");
    }

    #[test]
    fn globals_keep_clear_of_generated_names() {
        let model = "GltfPbr";
        for name in ["n12", "ctx", "s", "jit", "nscale", "shadewire_t0", "GltfPbr", "ShadewireContext"] {
            assert!(taken(name, model), "{name}");
        }
        for name in ["nx", "n1x", "base_color", "tint", "roughness"] {
            assert!(!taken(name, model), "{name}");
        }
    }
}
