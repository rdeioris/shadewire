//! The back end: any front end's IR to a Godot shader and reflection.

use shadewire::optimize::optimize;
use shadewire::{AddressMode, BindingOptions, Dag, Error, Reflection, SourceInfo, reflect};

use crate::emit::sample::repeats;
use crate::emit::{UniformNames, emit};
use crate::options::GdshaderOptions;

/// The result of a compilation: the `.gdshader` text, what a host must bind, the Godot name of
/// each reflection uniform, and the DAG the shader was emitted from (optimized when asked).
///
/// Godot binds by name: uniform `i` is the shader parameter `uniform_names[i]` (the authored name,
/// made a valid identifier clear of reserved and generated names) and texture `t` is
/// `shadewire_t<t.index>`. Reflection's `group` and bindings mean nothing in Godot.
#[derive(Clone, Debug)]
pub struct GdshaderOutput {
    pub code: String,
    pub reflection: Reflection,
    pub uniform_names: Vec<String>,
    pub dag: Dag,
}

/// Optimize (when asked), reflect, emit. Godot's approximations are appended to reflection's.
pub fn compile(dag: &Dag, source: &SourceInfo, options: &GdshaderOptions) -> Result<GdshaderOutput, Error> {
    let dag = if options.optimize { optimize(dag) } else { dag.clone() };
    let mut reflection = reflect(&dag, &BindingOptions { group: 0, first_binding: 0 }, source)?;
    let names = UniformNames::new(&reflection);
    let assembled = emit(&dag, &reflection, &names, options);
    let mut notes = assembled.approximations;
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
    Ok(GdshaderOutput { code: assembled.code, reflection, uniform_names: names.into_vec(), dag })
}
