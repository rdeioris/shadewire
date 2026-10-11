use super::reserved::RESERVED;

/// Makes an authored name a valid WGSL identifier: invalid characters become `_`,
/// a leading digit gets a `_` prefix, reserved words get a trailing `_`.
pub fn ident(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    // A lone `_` is the phony assignment token, and a `__` prefix is reserved.
    if out == "_" || out.starts_with("__") {
        out.insert(0, 'x');
    }
    if RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// The WGSL struct name of a shading model.
pub fn struct_name(model: &str) -> String {
    shadewire_ir::model_struct_name(model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_words_get_an_underscore() {
        assert_eq!(ident("filter"), "filter_");
        assert_eq!(ident("struct"), "struct_");
        assert_eq!(ident("base_color"), "base_color");
    }

    #[test]
    fn invalid_characters_are_replaced() {
        assert_eq!(ident("my-prop.x"), "my_prop_x");
        assert_eq!(ident("2d"), "_2d");
        assert_eq!(ident("a b"), "a_b");
        assert_eq!(ident("_"), "x_");
        assert_eq!(ident("__a"), "x__a");
    }

    #[test]
    fn struct_name_is_the_ir_one() {
        assert_eq!(struct_name("open_pbr_surface"), "OpenPbrSurface");
    }
}
