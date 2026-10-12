//! Helpers shared by the emitter's test modules.

use std::collections::BTreeSet;

use shadewire::{BindingOptions, Dag, Node, NodeId, NodeKind, Reflection, SourceInfo, Type, reflect};

pub fn add(dag: &mut Dag, kind: NodeKind, ty: Type) -> NodeId {
    dag.add(Node { kind, ty, origin: String::new() })
}

pub fn reflection(dag: &Dag) -> Reflection {
    let info = SourceInfo { source: "t".into(), mtlx_version: "1.39".into() };
    reflect(dag, &BindingOptions { group: 0, first_binding: 0 }, &info).unwrap()
}

const TYPES: &[&str] = &[
    "bool",
    "int",
    "uint",
    "float",
    "vec2",
    "vec3",
    "vec4",
    "uvec3",
    "bvec2",
    "bvec3",
    "bvec4",
    "mat3",
    "mat4",
    "sampler2D",
    "void",
];

fn words(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        let part = c.is_ascii_alphanumeric() || c == '_';
        match (part, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &text[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &text[s..]));
    }
    out
}

/// The Godot shading language rules naga cannot check for us, approximated on the text:
/// no WGSL left over, balanced brackets, functions defined before they are called, and no
/// parameter or local that redeclares a global (Godot reports "Redefinition"). The real check
/// is Godot's own compiler (`godot --headless`); this catches the emitter's own slips.
pub fn godot_rules(code: &str) {
    let text: String = code.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n");
    for wgsl in ["<f32>", "->", "@", "textureSample"] {
        assert!(!text.contains(wgsl), "WGSL `{wgsl}` in\n{code}");
    }
    for (_, word) in words(&text) {
        let wgsl = ["let", "var", "fn", "select", "bitcast", "dpdx", "dpdy", "atan2", "f32", "i32", "u32"];
        assert!(!wgsl.contains(&word), "WGSL `{word}` in\n{code}");
    }
    assert!(text.starts_with("shader_type spatial;\nrender_mode "), "{code}");
    for (open, close) in [('(', ')'), ('{', '}'), ('[', ']')] {
        assert_eq!(text.matches(open).count(), text.matches(close).count(), "unbalanced {open}{close} in\n{code}");
    }
    let mut structs: Vec<&str> = TYPES.to_vec();
    let mut globals: BTreeSet<&str> = BTreeSet::new();
    let mut defined: BTreeSet<&str> = BTreeSet::new();
    let mut depth = 0i32;
    let tokens = words(&text);
    let bytes = text.as_bytes();
    let after = |pos: usize, len: usize| text[pos + len..].trim_start().chars().next();
    let mut last_brace = 0;
    for (k, (pos, word)) in tokens.iter().enumerate() {
        for c in text[last_brace..*pos].chars() {
            depth += match c {
                '{' => 1,
                '}' => -1,
                _ => 0,
            };
        }
        last_brace = *pos;
        let prev = if k > 0 { tokens[k - 1].1 } else { "" };
        let preceded_by_dot = *pos > 0 && bytes[pos - 1] == b'.';
        if preceded_by_dot {
            continue;
        }
        if depth == 0 {
            if prev == "struct" {
                structs.push(word);
                globals.insert(word);
            } else if structs.contains(&prev) && after(*pos, word.len()) == Some('(') {
                assert!(globals.insert(word), "function {word} defined twice");
                defined.insert(word);
            } else if structs.contains(&prev) && k >= 2 && ["uniform", "varying", "const"].contains(&tokens[k - 2].1) {
                assert!(globals.insert(word), "global {word} declared twice");
            }
        }
        if word.starts_with("shadewire_") && after(*pos, word.len()) == Some('(') && !structs.contains(&prev) {
            assert!(defined.contains(word), "{word} called before it is defined in\n{code}");
        }
    }
    // Parameters and locals: a type name followed by a name that is not a call or a constructor,
    // inside a function's parameter list or body (struct bodies hold members, not locals).
    let mut depth = 0i32;
    let mut last = 0;
    let mut in_parameters = false;
    let mut in_struct = false;
    for (k, (pos, word)) in tokens.iter().enumerate() {
        for c in text[last..*pos].chars() {
            match c {
                '{' => {
                    depth += 1;
                    in_parameters = false;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        in_struct = false;
                    }
                }
                _ => {}
            }
        }
        last = *pos;
        let prev = if k > 0 { tokens[k - 1].1 } else { "" };
        if depth == 0 && prev == "struct" {
            in_struct = true;
        }
        if depth == 0 && structs.contains(&prev) && after(*pos, word.len()) == Some('(') {
            in_parameters = true;
            continue;
        }
        let declares = structs.contains(&prev) && after(*pos, word.len()) != Some('(');
        let local = (depth > 0 && !in_struct) || in_parameters;
        if declares && local && !(*pos > 0 && bytes[pos - 1] == b'.') {
            assert!(!globals.contains(word), "local or parameter {word} redeclares a global in\n{code}");
        }
    }
}

#[test]
fn godot_rules_catch_redefinitions_and_order() {
    let ok = "shader_type spatial;\nrender_mode cull_back;\nuniform float k = 1.0;\nfloat shadewire_a(float x) { return x * k; }\nvoid fragment() { float y = shadewire_a(1.0); }\n";
    godot_rules(ok);
    let shadow = ok.replace("float x", "float k").replace("x * k", "k");
    assert!(std::panic::catch_unwind(|| godot_rules(&shadow)).is_err());
    let order = "shader_type spatial;\nrender_mode cull_back;\nvoid fragment() { float y = shadewire_a(1.0); }\nfloat shadewire_a(float x) { return x; }\n";
    assert!(std::panic::catch_unwind(|| godot_rules(order)).is_err());
}
