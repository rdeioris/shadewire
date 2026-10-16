//! Identifiers and string literals as MDL reads them.

use shadewire::shader::names::ident;

/// MDL 1.7's reserved words (the specification's 5.7), both those in use and those reserved for the future.
pub const RESERVED: &[&str] = &[
    "annotation",
    "auto",
    "bool",
    "bool2",
    "bool3",
    "bool4",
    "break",
    "bsdf",
    "bsdf_measurement",
    "case",
    "cast",
    "color",
    "const",
    "continue",
    "declarative",
    "default",
    "do",
    "double",
    "double2",
    "double2x2",
    "double2x3",
    "double3",
    "double3x2",
    "double3x3",
    "double3x4",
    "double4",
    "double4x3",
    "double4x4",
    "double4x2",
    "double2x4",
    "edf",
    "else",
    "enum",
    "export",
    "false",
    "float",
    "float2",
    "float2x2",
    "float2x3",
    "float3",
    "float3x2",
    "float3x3",
    "float3x4",
    "float4",
    "float4x3",
    "float4x4",
    "float4x2",
    "float2x4",
    "for",
    "hair_bsdf",
    "if",
    "import",
    "in",
    "int",
    "int2",
    "int3",
    "int4",
    "intensity_mode",
    "intensity_power",
    "intensity_radiant_exitance",
    "let",
    "light_profile",
    "material",
    "material_emission",
    "material_geometry",
    "material_surface",
    "material_volume",
    "mdl",
    "module",
    "operator",
    "package",
    "return",
    "string",
    "struct",
    "struct_category",
    "switch",
    "texture_2d",
    "texture_3d",
    "texture_cube",
    "texture_ptex",
    "true",
    "typedef",
    "uniform",
    "using",
    "varying",
    "vdf",
    "while",
    "catch",
    "char",
    "class",
    "const_cast",
    "delete",
    "dynamic_cast",
    "explicit",
    "extern",
    "external",
    "foreach",
    "friend",
    "goto",
    "graph",
    "half",
    "half2",
    "half2x2",
    "half2x3",
    "half3",
    "half3x2",
    "half3x3",
    "half3x4",
    "half4",
    "half4x3",
    "half4x4",
    "half4x2",
    "half2x4",
    "inline",
    "inout",
    "lambda",
    "long",
    "mutable",
    "namespace",
    "native",
    "new",
    "out",
    "phenomenon",
    "private",
    "protected",
    "public",
    "reinterpret_cast",
    "sampler",
    "shader",
    "short",
    "signed",
    "sizeof",
    "static",
    "static_cast",
    "technique",
    "template",
    "this",
    "throw",
    "try",
    "typeid",
    "typename",
    "union",
    "unsigned",
    "virtual",
    "void",
    "volatile",
    "wchar_t",
    "df",
    "math",
    "state",
    "tex",
    "anno",
];

/// An authored name as an MDL identifier: valid in every language Shadewire writes (`ident`), then a trailing `_`
/// while it is an MDL reserved word or a standard module's name.
pub fn mdl_ident(name: &str) -> String {
    let mut out = ident(name);
    // MDL identifiers start with a letter.
    if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
        out.insert(0, 'x');
    }
    while RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// A file path as MDL names a resource: `/` separators, and no drive letter (MDL paths cannot hold `:`; an
/// absolute path is then read against the search path). The second value says the drive was dropped.
pub fn mdl_resource(path: &str) -> (String, bool) {
    let path = path.replace('\\', "/");
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        let rest = &path[2..];
        let rest = if rest.starts_with('/') { rest.to_string() } else { format!("/{rest}") };
        return (rest, true);
    }
    (path, false)
}

/// `s` as an MDL string literal; backslashes become `/`, MDL's path separator.
pub fn mdl_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push('/'),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
