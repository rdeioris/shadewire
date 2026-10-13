use shadewire::shader::names::struct_name;
use shadewire::shader::{CompileOptions, lower};
use shadewire::{AddressMode, Dag, Error, SourceInfo};

use crate::context::{context, host_members};
use crate::names::{SURFACE, uniform_names};
use crate::options::GdshaderOptions;
use crate::rewrite::{declared_names, for_godot};
use crate::surface::{HELPERS, Mapping, closure_mapping, map};
use crate::syntax::{print, repeats};
use crate::uniforms::uniform_block;
use crate::{GdshaderOutput, lobes};

fn header(mapping: &Mapping) -> String {
    format!("shader_type spatial;\nrender_mode {};\n", mapping.render_modes.join(", "))
}

/// Optimize (when asked), reflect, build the material's tree, and write it as a complete Godot shader: uniforms,
/// the shared material code, the context `fragment()` fills, and either the model mapped onto Godot's outputs or,
/// for a closure tree, a `light()` evaluating MaterialX's lobes.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &GdshaderOptions) -> Result<GdshaderOutput, Error> {
    let shared = CompileOptions {
        group: 0,
        first_binding: 0,
        reserve_uniform_binding: false,
        flip_v: true,
        optimize: options.optimize,
        implicit_derivatives: true,
    };
    let (mut module, mut reflection, dag) = lower(dag, source, &shared)?;
    let ctx = context(&dag, &reflection);
    let names = uniform_names(&reflection, &declared_names(&module));
    for_godot(&mut module, &reflection, &names, &host_members(&reflection), !ctx.members.is_empty());
    let closure = dag.surface.closure.is_some();
    let plan = closure.then(|| lobes::Plan::new(&dag, &reflection, &names));
    let mut mapping =
        if closure { closure_mapping(&dag, &reflection, options) } else { map(&dag, &reflection, options) };
    if closure {
        mapping.approximations.push(lobes::AMBIENT.to_string());
    }

    let mut sections = vec![header(&mapping), uniform_block(&reflection, &names)];
    if mapping.helpers {
        sections.push(HELPERS.to_string());
    }
    sections.push(ctx.struct_text());
    sections.push(print(&module, &reflection.textures));
    if let Some(plan) = &plan {
        sections.push(plan.varyings());
    }
    sections.push(ctx.vertex_text());
    sections.push(fragment(&dag, &ctx, &mapping, plan.as_ref(), options));
    if let Some(plan) = &plan {
        sections.push(plan.light(&dag));
    }
    let parts: Vec<&str> = sections.iter().map(|s| s.trim_end()).filter(|s| !s.is_empty()).collect();
    let mut code = parts.join("\n\n");
    code.push('\n');

    let mut notes = ctx.approximations;
    notes.extend(mapping.approximations);
    for t in &reflection.textures {
        if repeats(t) && t.address.iter().any(|a| matches!(a, AddressMode::Clamp | AddressMode::Constant)) {
            notes.push(format!("godot: {}: clamp emulated on a repeating sampler (edge texels wrap)", t.file));
        }
    }
    for note in notes {
        if !reflection.approximations.contains(&note) {
            reflection.approximations.push(note);
        }
    }
    Ok(GdshaderOutput { code, reflection, uniform_names: names, dag })
}

fn fragment(
    dag: &Dag,
    ctx: &crate::context::Context,
    mapping: &Mapping,
    plan: Option<&lobes::Plan>,
    options: &GdshaderOptions,
) -> String {
    let mut out = String::from("void fragment() {\n");
    let (fill, argument) = ctx.fill();
    for line in fill {
        out.push_str(&format!("    {line}\n"));
    }
    let name = struct_name(&dag.surface.model);
    out.push_str(&format!("    {name} {SURFACE} = shadewire_eval_surface({argument});\n"));
    let lobe_lines = plan.map_or_else(Vec::new, |p| p.fragment_lines(options.emission_scale));
    for line in lobe_lines.iter().chain(&mapping.lines) {
        out.push_str(&format!("    {line}\n"));
    }
    out.push_str("}\n");
    out
}
