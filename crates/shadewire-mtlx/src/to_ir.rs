//! The MaterialX front end in one call: text to the IR.

use shadewire::{Dag, Error, SourceInfo};

use crate::include::{Resolver, resolve_includes};
use crate::upgrade::upgrade;
use shadewire_nodes::LowerOptions;
use shadewire_nodes::document::build::build;
use shadewire_nodes::elaborate::elaborate;
use shadewire_nodes::library::standard;
use shadewire_nodes::parse::parse_str;

/// One material lowered to the IR, with where it came from (for reflection).
#[derive(Clone, Debug)]
pub struct MaterialIr {
    pub dag: Dag,
    pub source: SourceInfo,
}

/// MaterialX text to the IR: parse, includes, build, upgrade, elaborate. Not optimized.
/// `location` names the document (in messages, and as the base `resolver` joins includes to): an OS path, or any
/// string the resolver understands. `resolver` joins and reads each `xi:include`; `Resolver::default()` reads the
/// file system.
pub fn to_ir(
    text: &str,
    location: Option<&str>,
    material: Option<&str>,
    options: &LowerOptions,
    resolver: &Resolver,
) -> Result<MaterialIr, Error> {
    let root = parse_str(text, location)?;
    let root = resolve_includes(root, location, resolver)?;
    let document = build(&root, location)?;
    let (major, minor) = document.version;
    let document = upgrade(document)?;
    let dag = elaborate(&document, standard(), options, material)?;
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
        let ir = to_ir(&text, Some(DEFAULT), None, &LowerOptions::default(), &Resolver::default()).unwrap();
        assert_eq!(ir.dag.surface.model, "standard_surface");
        assert_eq!(ir.source.mtlx_version, "1.39");
        assert!(ir.dag.dump().starts_with("model standard_surface\n"), "{}", ir.dag.dump());
    }

    #[test]
    fn a_model_the_options_refuse_is_no_translation() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../vendor/materialx/resources/Materials/Examples/OpenPbr/open_pbr_default.mtlx"
        ))
        .unwrap();
        let options = LowerOptions::with_models(&["standard_surface"]);
        let err = to_ir(&text, None, None, &options, &Resolver::default()).unwrap_err();
        assert!(matches!(err, Error::NoTranslation { .. }), "{err}");
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
        let err = to_ir(text, None, None, &LowerOptions::default(), &Resolver::default()).unwrap_err();
        assert!(err.to_string().contains("includes need a document location"), "{err}");
    }
}
