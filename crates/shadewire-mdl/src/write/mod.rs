//! The IR as a self-contained MDL 1.7 module: the shared shader tree printed with an MDL syntax, the surface as
//! a `material` whose parameters are the uniforms and textures.

pub mod df;
pub mod module;
pub mod names;
pub mod syntax;

#[cfg(test)]
mod syntax_tests;

use shadewire::shader::names::{ident, struct_name};
use shadewire::shader::{CompileOptions, Compiled, lower};
use shadewire::{Dag, Error, NodeKind, SourceInfo};

pub use names::{mdl_ident, mdl_string};

/// The two models MDL writes: a closure tree, or unlit emission.
pub const MODELS: [&str; 2] = ["surface", "surface_unlit"];

/// What the MDL back end writes.
#[derive(Clone, Debug, PartialEq)]
pub struct MdlOptions {
    /// The exported material's name, made a valid MDL identifier.
    pub name: String,
    /// Run the optimizer before writing.
    pub optimize: bool,
}

impl Default for MdlOptions {
    fn default() -> MdlOptions {
        MdlOptions { name: "material".to_string(), optimize: true }
    }
}

/// The local holding `shadewire_eval_surface`'s result in the material's `let`.
const SURFACE: &str = "s";

/// The module's first lines.
const HEADER: &str =
    "mdl 1.7;\n\nimport ::anno::*;\nimport ::df::*;\nimport ::math::*;\nimport ::state::*;\nimport ::tex::*;\n";

/// Writes `dag` as an MDL module exporting one material; `NoTranslation` for a model other than a closure tree or
/// `surface_unlit`.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &MdlOptions) -> Result<Compiled, Error> {
    let model = dag.surface.model.clone();
    if !MODELS.contains(&model.as_str()) {
        return Err(Error::NoTranslation { from: model, to: MODELS.iter().map(|m| m.to_string()).collect() });
    }
    let shared =
        CompileOptions { flip_v: false, implicit_derivatives: false, optimize: options.optimize, ..Default::default() };
    let (_, mut reflection, dag) = lower(dag, source, &shared)?;
    let (module, params) = module::build(&dag, &mut reflection);
    let (lets, material) = match &dag.surface.closure {
        None => unlit(&dag, &mut reflection),
        Some(graph) => {
            let uniforms: Vec<(String, String)> =
                reflection.uniforms.iter().zip(&params).map(|(u, p)| (u.name.clone(), p.name.clone())).collect();
            let uniform_param = |name: &str| uniforms.iter().find(|(u, _)| u == name).map(|(_, p)| p.clone());
            let mut closures = df::Closures::new(&dag, graph, &uniform_param);
            let material = closures.material();
            for note in std::mem::take(&mut closures.notes) {
                if !reflection.approximations.contains(&note) {
                    reflection.approximations.push(note);
                }
            }
            (closures.lines, material)
        }
    };
    let mut code = String::from(HEADER);
    code.push('\n');
    code.push_str(&syntax::print(&module));
    code.push('\n');
    let declared: Vec<String> = params
        .iter()
        .map(|p| {
            let annotations =
                if p.annotations.is_empty() { String::new() } else { format!(" [[ {} ]]", p.annotations.join(", ")) };
            format!("    {} {} = {}{annotations}", p.mdl_ty, p.name, p.default)
        })
        .collect();
    let arguments: Vec<&str> = params.iter().map(|p| p.argument.as_str()).collect();
    let declared = if declared.is_empty() { String::new() } else { format!("\n{}\n", declared.join(",\n")) };
    code.push_str(&format!("export material {}({declared}) = let {{\n", mdl_ident(&options.name)));
    code.push_str(&format!(
        "    {} {SURFACE} = shadewire_eval_surface({});\n",
        struct_name(&dag.surface.model),
        arguments.join(", ")
    ));
    for line in lets {
        code.push_str(&format!("    {line}\n"));
    }
    code.push_str(&format!("}} in {material};\n"));
    Ok(Compiled { code, reflection, dag })
}

/// The surface field `name` of the local, as MDL source.
fn field(name: &str) -> String {
    format!("{SURFACE}.{}", ident(name))
}

/// `surface_unlit`: emission and cutout opacity; transmission has no place in an emission-only material.
fn unlit(dag: &Dag, reflection: &mut shadewire::Reflection) -> (Vec<String>, String) {
    let constant_zero = |name: &str| {
        dag.surface.fields.iter().find(|f| f.name == name).is_none_or(
            |f| matches!(&dag.node(f.node).kind, NodeKind::Const(v) if v.floats().iter().all(|x| *x == 0.0)),
        )
    };
    if !constant_zero("transmission") {
        reflection.approximations.push("mdl: surface_unlit transmission dropped".to_string());
    }
    let intensity = format!("color({} * {} * math::PI)", field("emission_color"), field("emission"));
    let material = format!(
        "material(surface: material_surface(emission: material_emission(emission: df::diffuse_edf(), intensity: {intensity})), \
         geometry: material_geometry(cutout_opacity: {}))",
        field("opacity")
    );
    (Vec::new(), material)
}
