use shadewire::{Dag, Reflection};

use super::context::{Context, context};
use super::expressions::{node_lets, node_name};
use super::names::{SURFACE, UniformNames, ident, struct_name};
use super::ops::cast;
use super::surface::{HELPERS, Mapping, map};
use super::types::gd_type;
use crate::leaf_library::files_for;
use crate::options::GdshaderOptions;

/// The generated shader and what it adds to reflection's approximations.
pub struct Assembled {
    pub code: String,
    pub approximations: Vec<String>,
}

/// Joins the sections, skipping empty ones, one blank line between them: header, uniforms,
/// leaf library (only the files the live nodes call, each once, in dependency order), helpers,
/// context and model structs, the entry, `vertex()` when a varying is needed, `fragment()`.
pub fn assemble(dag: &Dag, reflection: &Reflection, names: &UniformNames, options: &GdshaderOptions) -> Assembled {
    let ctx = context(dag, reflection);
    let mapping = map(dag, reflection, options);
    let mut sections = vec![header(&mapping), super::uniforms::uniform_block(reflection, names)];
    sections.extend(files_for(dag).into_iter().map(String::from));
    if mapping.helpers {
        sections.push(HELPERS.to_string());
    }
    sections.push(ctx.struct_text());
    sections.push(model_struct(dag));
    sections.push(entry(dag, reflection, names, &ctx));
    sections.push(ctx.vertex_text());
    sections.push(fragment(dag, &ctx, &mapping));
    let parts: Vec<&str> = sections.iter().map(|s| s.trim_end()).filter(|s| !s.is_empty()).collect();
    let mut code = parts.join("\n\n");
    code.push('\n');
    let mut approximations = ctx.approximations;
    for note in mapping.approximations {
        if !approximations.contains(&note) {
            approximations.push(note);
        }
    }
    Assembled { code, approximations }
}

fn header(mapping: &Mapping) -> String {
    format!("shader_type spatial;\nrender_mode {};\n", mapping.render_modes.join(", "))
}

/// The struct the entry function returns: one member per surface field.
pub fn model_struct(dag: &Dag) -> String {
    let mut out = format!("struct {} {{\n", struct_name(&dag.surface.model));
    for field in &dag.surface.fields {
        out.push_str(&format!("    {} {};\n", gd_type(field.ty), ident(&field.name)));
    }
    out.push_str("};\n");
    out
}

/// `{Model} shadewire_eval_surface(ShadewireContext ctx)`: the live node values, then the
/// struct in field order.
pub fn entry(dag: &Dag, reflection: &Reflection, names: &UniformNames, ctx: &Context) -> String {
    let name = struct_name(&dag.surface.model);
    let mut out = format!("{name} shadewire_eval_surface({}) {{\n", ctx.parameter());
    for line in node_lets(dag, reflection, names) {
        out.push_str(&format!("    {line}\n"));
    }
    let fields: Vec<String> =
        dag.surface.fields.iter().map(|f| cast(&node_name(f.node), dag.node(f.node).ty, f.ty)).collect();
    out.push_str(&format!("    return {name}({});\n}}\n", fields.join(", ")));
    out
}

fn fragment(dag: &Dag, ctx: &Context, mapping: &Mapping) -> String {
    let mut out = String::from("void fragment() {\n");
    let (fill, argument) = ctx.fill();
    for line in fill {
        out.push_str(&format!("    {line}\n"));
    }
    let name = struct_name(&dag.surface.model);
    out.push_str(&format!("    {name} {SURFACE} = shadewire_eval_surface({argument});\n"));
    for line in &mapping.lines {
        out.push_str(&format!("    {line}\n"));
    }
    out.push_str("}\n");
    out
}
