//! What the builder is told: which material, the MaterialX front end's options and the compile knobs the
//! Bevy layout leaves open.

use shadewire_mtlx::MtlxOptions;
use shadewire_wgsl::CompileOptions;

use crate::shader::compile_options;

/// The models the builder accepts by default: the four it writes onto Bevy's `StandardMaterial` inputs, and
/// `surface`, so any other material (a raw BSDF graph, `UsdPreviewSurface`, `disney_principled`) opens into its
/// closure tree, lit by MaterialX's lobes.
pub const MODELS: [&str; 5] = ["open_pbr_surface", "standard_surface", "gltf_pbr", "surface_unlit", "surface"];

fn models(list: &[&str]) -> Vec<String> {
    list.iter().map(|m| m.to_string()).collect()
}

/// Options of [`FromMtlx`](crate::FromMtlx). `Default` builds the document's only material with
/// `MtlxOptions::default()` (values as uniforms) accepting [`MODELS`], optimized, V flipped.
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
            mtlx: MtlxOptions { models: models(&MODELS), ..MtlxOptions::default() },
            optimize: compile.optimize,
            flip_v: compile.flip_v,
        }
    }
}

impl BuildOptions {
    /// The default options accepting only closure trees (`shadewire::LOBE_MODELS`): every material is lit by
    /// MaterialX's lobes, named models opened through their implementation graphs.
    pub fn lobes() -> BuildOptions {
        let mut options = BuildOptions::default();
        options.mtlx.models = models(&shadewire::LOBE_MODELS);
        options
    }

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
    fn surface_is_accepted_by_default_and_alone_with_lobes() {
        let default = BuildOptions::default().mtlx.models;
        assert!(default.iter().any(|m| m == "surface") && default.iter().any(|m| m == "standard_surface"));
        assert_eq!(BuildOptions::lobes().mtlx.models, ["surface", "surface_unlit"]);
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
