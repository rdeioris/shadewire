//! What every front end decides about the IR it produces: which models the host accepts and how values are read.

/// The models most hosts map; `surface::KNOWN_MODELS` has two more, and `surface::CLOSURE_MODEL` opens every
/// model not listed into its closure tree.
pub const DEFAULT_MODELS: [&str; 4] = ["open_pbr_surface", "standard_surface", "gltf_pbr", "surface_unlit"];

#[derive(Clone, Debug, PartialEq)]
pub struct IrOptions {
    /// Surface models the host accepts; a material of another model is `Error::NoTranslation`.
    pub models: Vec<String>,
    /// The colorspace authored colors are converted to.
    pub working_colorspace: String,
    /// Authored surface values and top-level interface values become uniforms.
    pub values_as_uniforms: bool,
    /// For untrusted documents: the most IR nodes lowering may create before it fails with `Error::Limit`.
    pub max_nodes: Option<usize>,
}

impl Default for IrOptions {
    fn default() -> IrOptions {
        IrOptions::with_models(&DEFAULT_MODELS)
    }
}

impl IrOptions {
    /// The defaults, accepting `models`.
    pub fn with_models(models: &[&str]) -> IrOptions {
        IrOptions {
            models: models.iter().map(|m| m.to_string()).collect(),
            working_colorspace: "lin_rec709".to_string(),
            values_as_uniforms: true,
            max_nodes: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_four_models_most_hosts_map() {
        let o = IrOptions::default();
        assert_eq!(o.models, DEFAULT_MODELS.iter().map(|m| m.to_string()).collect::<Vec<_>>());
        assert_eq!(o.working_colorspace, "lin_rec709");
        assert!(o.values_as_uniforms);
        assert_eq!(o.max_nodes, None);
    }

    #[test]
    fn with_models_keeps_the_other_defaults() {
        let o = IrOptions::with_models(&["surface"]);
        assert_eq!(o.models, ["surface"]);
        assert!(o.values_as_uniforms);
    }
}
