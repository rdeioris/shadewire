//! A compiled material as a GLSL library for hosts written in GLSL: the surface struct, the material's uniform
//! block and samplers, `shadewire_eval_surface()`, and a prototype for every host function it calls. The host
//! appends its own GLSL (the host functions, its lighting, `main`) and compiles the whole as a fragment shader.
//!
//! naga writes only complete stages, so the generated module is linked with stub host functions and an entry
//! point, translated, and the stubs become prototypes again; names naga suffixed with `_` (those ending in a
//! digit, such as `shadewire_host_texcoord_0` or a `c3_color0` field) get their reflection names back.

use shadewire::{HOST_PREFIX, Reflection, model_struct_name};

use crate::emit::names::ident;
use crate::emit::types::wgsl_type;
use crate::link::link;
use crate::target::{GlslVersion, Target};
use crate::translate::{Output, translate};
use shadewire::Error;

const ENTRY: &str = "shadewire_library_main";

/// The host functions the reflection lists: `(name, WGSL stub)`.
fn stubs(reflection: &Reflection) -> Vec<(String, String)> {
    let context = &reflection.context;
    let mut out = Vec::new();
    for entry in context.varyings.iter().chain(&context.derived).chain(&context.globals) {
        let name = format!("{HOST_PREFIX}{}", entry.name);
        let ty = wgsl_type(entry.ty);
        out.push((name.clone(), format!("fn {name}() -> {ty} {{ return {ty}(); }}\n")));
    }
    for g in &context.geomprops {
        let ty = wgsl_type(g.ty);
        out.push((g.function.clone(), format!("fn {}() -> {ty} {{ return {ty}(); }}\n", g.function)));
    }
    for t in &context.transforms {
        let name = format!("{HOST_PREFIX}{t}");
        out.push((name.clone(), format!("fn {name}(v: vec3<f32>) -> vec3<f32> {{ return v; }}\n")));
    }
    out
}

/// Whether `word` occurs at `at` in `text` as a whole identifier.
fn whole_word(text: &str, at: usize, word: &str) -> bool {
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let before = text[..at].chars().next_back().is_none_or(|c| !ident(c));
    let after = text[at + word.len()..].chars().next().is_none_or(|c| !ident(c));
    before && after
}

/// `text` with whole-identifier `from` replaced by `to`.
fn rename(text: &str, from: &str, to: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    let mut search = 0;
    while let Some(found) = text[search..].find(from) {
        let at = search + found;
        if whole_word(text, at, from) {
            out.push_str(&text[rest..at]);
            out.push_str(to);
            rest = at + from.len();
        }
        search = at + from.len();
    }
    out.push_str(&text[rest..]);
    out
}

/// Drops the definition starting at line `start` (through its closing `}` at column 0); returns the line after.
fn definition_end(lines: &[&str], start: usize) -> usize {
    let mut i = start;
    while i < lines.len() && lines[i] != "}" {
        i += 1;
    }
    i + 1
}

/// The material as a GLSL library (see the module doc). `generated` is `Compiled::wgsl`.
pub fn glsl_library(generated: &str, reflection: &Reflection, version: GlslVersion) -> Result<String, Error> {
    let stubs = stubs(reflection);
    let mut prelude: String = stubs.iter().map(|(_, code)| code.as_str()).collect();
    prelude.push_str(&format!(
        "@fragment fn {ENTRY}() -> @location(0) vec4<f32> {{ _ = shadewire_eval_surface(); return vec4<f32>(0.0); }}\n"
    ));
    let linked = link(generated, &prelude)?;
    let Output::Text(glsl) = translate(&linked, Target::Glsl(version), Some(ENTRY))? else {
        return Err(Error::Naga("GLSL is text".into()));
    };

    // naga suffixes `_` to identifiers ending in a digit: the names a host writes get theirs back.
    let mut names: Vec<String> = stubs.iter().map(|(name, _)| name.clone()).collect();
    names.extend(reflection.params.keys().map(|field| ident(field)));
    let mut glsl = glsl;
    for name in &names {
        if name.ends_with(|c: char| c.is_ascii_digit()) {
            glsl = rename(&glsl, &format!("{name}_"), name);
        }
    }

    // Stubs become prototypes; the entry point and its output go.
    let stub_names: Vec<&str> = stubs.iter().map(|(name, _)| name.as_str()).collect();
    let lines: Vec<&str> = glsl.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let defines = |name: &str| line.contains(&format!(" {name}(")) && line.ends_with(") {");
        if line.starts_with("void main()") {
            i = definition_end(&lines, i);
            continue;
        }
        if line.starts_with("layout(location = 0) out ") {
            i += 1;
            continue;
        }
        if !line.starts_with(' ') && stub_names.iter().any(|name| defines(name)) {
            out.push_str(line.trim_end_matches(" {"));
            out.push_str(";\n");
            i = definition_end(&lines, i);
            continue;
        }
        out.push_str(line);
        out.push('\n');
        i += 1;
    }
    let model = model_struct_name(&reflection.model);
    out.push_str(&format!(
        "// The host defines the shadewire_host_* functions above and calls `{model} shadewire_eval_surface()`.\n"
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit::test_support::{add, reflection};
    use crate::{EmitOptions, emit};
    use shadewire::*;

    fn material() -> (String, Reflection) {
        let mut dag = Dag::new("surface");
        let uv = add(&mut dag, NodeKind::Context(ContextRead::TexCoord { index: 0, ty: Type::Vector2 }), Type::Vector2);
        let n = add(&mut dag, NodeKind::Context(ContextRead::Normal(Space::World)), Type::Vector3);
        let x = add(&mut dag, NodeKind::Op { op: Op::Extract(0), args: vec![uv] }, Type::Float);
        let c = add(&mut dag, NodeKind::Op { op: Op::Convert, args: vec![x] }, Type::Color3);
        dag.surface.fields.push(SurfaceField { name: "c0_color0".into(), ty: Type::Color3, node: c });
        dag.surface.fields.push(SurfaceField { name: "c0_normal".into(), ty: Type::Vector3, node: n });
        let r = reflection(&dag);
        (emit(&dag, &r, &EmitOptions::default()), r)
    }

    #[test]
    fn the_library_declares_the_host_functions_and_keeps_reflection_names() {
        let (generated, r) = material();
        let glsl = glsl_library(&generated, &r, GlslVersion::Desktop(420)).unwrap();
        assert!(glsl.starts_with("#version 420 core\n"), "{glsl}");
        assert!(glsl.contains("vec2 shadewire_host_texcoord_0();\n"), "{glsl}");
        assert!(glsl.contains("vec3 shadewire_host_normal_world();\n"), "{glsl}");
        assert!(glsl.contains("vec3 c0_color0;"), "{glsl}");
        assert!(glsl.contains("Surface shadewire_eval_surface() {"), "{glsl}");
        assert!(!glsl.contains("main()") && !glsl.contains("_fs2p_"), "{glsl}");
        assert!(!glsl.contains("texcoord_0_") && !glsl.contains("color0_"), "{glsl}");
    }

    #[test]
    fn rename_touches_whole_identifiers_only() {
        assert_eq!(rename("a_0_ + ba_0_ + a_0_x", "a_0_", "a_0"), "a_0 + ba_0_ + a_0_x");
    }
}
