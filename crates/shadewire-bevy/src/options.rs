//! What the builder is told: which material, the MaterialX front end's options and the compile knobs the
//! Bevy layout leaves open.

use shadewire_mtlx::MtlxOptions;
use shadewire_wgsl::CompileOptions;

use crate::shader::compile_options;

/// Options of [`FromMtlx`](crate::FromMtlx). `Default` builds the document's only material with
/// `MtlxOptions::default()` (values as uniforms), optimized, V flipped.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildOptions {
    /// The material to build; `None` builds the document's only material, and is an error when it has several.
    pub material: Option<String>,
    /// The MaterialX front end's options: accepted models, working colour space, `values_as_uniforms`.
    pub mtlx: MtlxOptions,
    /// Run the optimizer before reflection and emission.
    pub optimize: bool,
    /// Flip V when sampling: Bevy uploads images top row first.
    pub flip_v: bool,
}

impl Default for BuildOptions {
    fn default() -> BuildOptions {
        let compile = compile_options();
        BuildOptions {
            material: None,
            mtlx: MtlxOptions::default(),
            optimize: compile.optimize,
            flip_v: compile.flip_v,
        }
    }
}

impl BuildOptions {
    /// The default options building the material `name`.
    pub fn material(name: impl Into<String>) -> BuildOptions {
        BuildOptions { material: Some(name.into()), ..BuildOptions::default() }
    }

    /// [`compile_options`] with this builder's `optimize` and `flip_v`; the bindings stay the extension's.
    pub fn compile_options(&self) -> CompileOptions {
        CompileOptions { optimize: self.optimize, flip_v: self.flip_v, ..compile_options() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_compiles_as_the_layout_does() {
        assert_eq!(BuildOptions::default().compile_options(), compile_options());
    }

    #[test]
    fn knobs_reach_the_compile_options_but_not_the_bindings() {
        let o = BuildOptions { optimize: false, flip_v: false, ..BuildOptions::material("M") };
        let c = o.compile_options();
        assert_eq!(o.material.as_deref(), Some("M"));
        assert_eq!((c.optimize, c.flip_v), (false, false));
        assert_eq!((c.group, c.first_binding, c.reserve_uniform_binding), (3, 100, true));
    }
}
