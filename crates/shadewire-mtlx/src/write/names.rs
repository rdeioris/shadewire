//! MaterialX element names from IR origins.

use std::collections::HashSet;

/// Turns an IR origin into a valid MaterialX name: the last `/` component with every character
/// outside `[A-Za-z0-9_]` replaced by `_`, a leading digit prefixed with `_`, and `node` when empty.
pub fn sanitize(origin: &str) -> String {
    sanitize_name(origin.rsplit('/').next().unwrap_or(""))
}

/// Turns a whole name (a material's, a uniform's) into a valid MaterialX name: every character outside
/// `[A-Za-z0-9_]` replaced by `_`, a leading digit prefixed with `_`, and `node` when empty.
pub fn sanitize_name(name: &str) -> String {
    let mut name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if name.is_empty() {
        return "node".to_string();
    }
    if name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    name
}

/// The names handed out within one scope, so each is unique.
#[derive(Debug, Default)]
pub struct Names {
    used: HashSet<String>,
}

impl Names {
    /// An empty set of names.
    pub fn new() -> Names {
        Names::default()
    }

    /// A free name from `sanitize(wanted)`, suffixed `_2`, `_3`, ... until unused; reserves it.
    pub fn unique(&mut self, wanted: &str) -> String {
        let base = sanitize(wanted);
        let mut name = base.clone();
        let mut n = 2;
        while self.used.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        self.used.insert(name.clone());
        name
    }

    /// Claims an exact name; false when it is already taken.
    pub fn reserve(&mut self, exact: &str) -> bool {
        self.used.insert(exact.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitized_and_unique() {
        assert_eq!(sanitize("NG_marble/noise1"), "noise1");
        assert_eq!(sanitize("Principled BSDF.Base Color"), "Principled_BSDF_Base_Color");
        assert_eq!(sanitize("3d"), "_3d");
        assert_eq!(sanitize(""), "node");
        let mut names = Names::new();
        assert_eq!(names.unique("g/o"), "o");
        assert_eq!(names.unique("h/o"), "o_2");
        assert_eq!(names.unique("o"), "o_3");
        assert!(names.reserve("fresh"));
        assert!(!names.reserve("fresh"));
        assert_eq!(names.unique("fresh"), "fresh_2");
    }

    #[test]
    fn a_whole_name_is_sanitized() {
        assert_eq!(sanitize_name("my material/v2"), "my_material_v2");
        assert_eq!(sanitize_name("Material.001"), "Material_001");
        assert_eq!(sanitize_name("2nd"), "_2nd");
        assert_eq!(sanitize_name(""), "node");
    }
}
