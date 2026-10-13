//! The options the runtime API, the resource loader and the importer share.

use shadewire_gdshader::{GdshaderOptions, LOBE_MODELS, MODELS, Transparency};
use shadewire_mtlx::{MtlxOptions, Resolver};

/// Which surface models the front end accepts, and so how the material is lit.
#[derive(Clone, Debug, PartialEq)]
pub enum Models {
    /// Named models mapped onto Godot's outputs and lit by Godot's BRDF (`shadewire_gdshader::MODELS`).
    Mapped,
    /// Every material opened into its closure tree and lit by MaterialX's lobes (`shadewire_gdshader::LOBE_MODELS`).
    Lobes,
    /// An explicit list, as `MtlxOptions::models`.
    List(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadOptions {
    /// The material to load; the document's first when `None`.
    pub material: Option<String>,
    pub models: Models,
    /// Authored values become constants the optimizer can fold (the CLI's `--constants`) instead of uniforms.
    pub bake_constants: bool,
    pub optimize: bool,
    pub transparency: Transparency,
    pub emission_scale: f32,
    /// A texture that cannot be loaded fails the load instead of binding Godot's default and warning.
    pub strict_textures: bool,
}

impl Default for LoadOptions {
    fn default() -> LoadOptions {
        LoadOptions {
            material: None,
            models: Models::Mapped,
            bake_constants: false,
            optimize: true,
            transparency: Transparency::Auto,
            emission_scale: 1.0,
            strict_textures: false,
        }
    }
}

/// The values of the `transparency` option, in the importer's enum order.
pub const TRANSPARENCY: [(&str, Transparency); 5] = [
    ("auto", Transparency::Auto),
    ("opaque", Transparency::Opaque),
    ("scissor", Transparency::Scissor),
    ("hash", Transparency::Hash),
    ("blend", Transparency::Blend),
];

pub fn transparency(name: &str) -> Result<Transparency, String> {
    TRANSPARENCY
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .ok_or_else(|| format!("transparency \"{name}\": expected one of auto, opaque, scissor, hash, blend"))
}

pub fn models(name: &str) -> Result<Models, String> {
    match name {
        "mapped" => Ok(Models::Mapped),
        "lobes" => Ok(Models::Lobes),
        _ => Err(format!("models \"{name}\": expected \"mapped\", \"lobes\" or a list of model names")),
    }
}

impl LoadOptions {
    pub fn mtlx(&self, resolver: Resolver) -> MtlxOptions {
        let models = match &self.models {
            Models::Mapped => MODELS.iter().map(|m| m.to_string()).collect(),
            Models::Lobes => LOBE_MODELS.iter().map(|m| m.to_string()).collect(),
            Models::List(list) => list.clone(),
        };
        MtlxOptions { models, values_as_uniforms: !self.bake_constants, resolver, ..MtlxOptions::default() }
    }

    pub fn gdshader(&self) -> GdshaderOptions {
        GdshaderOptions {
            optimize: self.optimize,
            transparency: self.transparency,
            emission_scale: self.emission_scale,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_names_parse_and_reject_typos() {
        assert_eq!(transparency("hash").unwrap(), Transparency::Hash);
        assert!(transparency("Hash").unwrap_err().contains("expected one of"));
        assert_eq!(models("lobes").unwrap(), Models::Lobes);
        assert!(models("lobe").is_err());
    }

    #[test]
    fn options_reach_both_halves() {
        let options = LoadOptions { models: Models::Lobes, bake_constants: true, ..LoadOptions::default() };
        let mtlx = options.mtlx(Resolver::default());
        assert_eq!(mtlx.models, ["surface", "surface_unlit"]);
        assert!(!mtlx.values_as_uniforms);
        assert_eq!(options.gdshader(), GdshaderOptions::default());
    }
}
