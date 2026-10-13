//! The MaterialX front end in one call: text to the IR.

use shadewire::{Dag, Error, SourceInfo};

use crate::document::build::build;
use crate::elaborate::{ElaborateOptions, elaborate};
use crate::include::{Resolver, resolve_includes};
use crate::library::standard;
use crate::parse::parse_str;
use crate::upgrade::upgrade;

/// What the front end decides: which models the host accepts and how values are read.
#[derive(Clone, Debug, PartialEq)]
pub struct MtlxOptions {
    /// Surface models the host accepts; a material of another model is `NoTranslation`. The default is the four
    /// models most hosts map; `surface::KNOWN_MODELS` also has `UsdPreviewSurface` and `disney_principled`, and
    /// `surface::CLOSURE_MODEL` opens every model not listed into its closure tree.
    pub models: Vec<String>,
    pub working_colorspace: String,
    /// Authored surface values and top-level interface values become uniforms.
    pub values_as_uniforms: bool,
    /// Joins and reads `xi:include`s; the default reads the file system.
    pub resolver: Resolver,
    /// For untrusted documents: the most IR nodes elaboration may create before it fails with `Error::Limit`.
    /// Checked as the graph grows, so nodegraphs instanced inside one another cannot blow up first.
    pub max_nodes: Option<usize>,
}

impl Default for MtlxOptions {
    fn default() -> MtlxOptions {
        MtlxOptions {
            models: ["open_pbr_surface", "standard_surface", "gltf_pbr", "surface_unlit"]
                .iter()
                .map(|m| m.to_string())
                .collect(),
            working_colorspace: "lin_rec709".to_string(),
            values_as_uniforms: true,
            resolver: Resolver::default(),
            max_nodes: None,
        }
    }
}

/// One material lowered to the IR, with where it came from (for reflection).
#[derive(Clone, Debug)]
pub struct MaterialIr {
    pub dag: Dag,
    pub source: SourceInfo,
}

/// MaterialX text to the IR: parse, includes, build, upgrade, elaborate. Not optimized.
/// `location` names the document (in messages, and as the base `options.resolver` joins includes to): an OS path,
/// or any string the resolver understands.
pub fn to_ir(
    text: &str,
    location: Option<&str>,
    material: Option<&str>,
    options: &MtlxOptions,
) -> Result<MaterialIr, Error> {
    let root = parse_str(text, location)?;
    let root = resolve_includes(root, location, &options.resolver)?;
    let document = build(&root, location)?;
    let (major, minor) = document.version;
    let document = upgrade(document)?;

    let elaborate_options = ElaborateOptions {
        working_colorspace: options.working_colorspace.clone(),
        values_as_uniforms: options.values_as_uniforms,
        max_nodes: options.max_nodes,
    };
    let models: Vec<&str> = options.models.iter().map(String::as_str).collect();
    let dag = elaborate(&document, standard(), &elaborate_options, material, &models)?;
    let source = SourceInfo { source: "mtlx".to_string(), mtlx_version: format!("{major}.{minor}") };
    Ok(MaterialIr { dag, source })
}

/// The names of the document's materials (`surfacematerial` nodes), in document order.
pub fn material_names(text: &str, location: Option<&str>, resolver: &Resolver) -> Result<Vec<String>, Error> {
    let root = parse_str(text, location)?;
    let root = resolve_includes(root, location, resolver)?;
    let document = build(&root, location)?;
    Ok(document.nodes.iter().filter(|n| n.category == "surfacematerial").map(|n| n.name.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_default.mtlx"
    );

    #[test]
    fn an_example_lowers_to_a_printable_dag() {
        let text = std::fs::read_to_string(DEFAULT).unwrap();
        let ir = to_ir(&text, Some(DEFAULT), None, &MtlxOptions::default()).unwrap();
        assert_eq!(ir.dag.surface.model, "standard_surface");
        assert_eq!(ir.source.mtlx_version, "1.39");
        assert!(ir.dag.dump().starts_with("model standard_surface\n"), "{}", ir.dag.dump());
    }

    #[test]
    fn material_names_are_listed_in_document_order() {
        let text = r#"<materialx version="1.39">
          <surfacematerial name="b" type="material"/><surfacematerial name="a" type="material"/></materialx>"#;
        assert_eq!(material_names(text, None, &Resolver::default()).unwrap(), ["b", "a"]);
    }

    #[test]
    fn includes_without_a_file_are_an_error() {
        let text = r#"<materialx version="1.39"><xi:include href="other.mtlx"/></materialx>"#;
        let err = to_ir(text, None, None, &MtlxOptions::default()).unwrap_err();
        assert!(err.to_string().contains("includes need a document location"), "{err}");
    }
}
