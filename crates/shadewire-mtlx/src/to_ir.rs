//! The MaterialX front end in one call: text to the IR.

use std::path::Path;

use shadewire_ir::{Dag, Error, Location, SourceInfo};

use crate::document::build::build;
use crate::elaborate::{ElaborateOptions, elaborate};
use crate::include::resolve_includes;
use crate::library::standard;
use crate::parse::parse_str;
use crate::upgrade::upgrade;

/// What the front end decides: which models the host accepts and how values are read.
#[derive(Clone, Debug, PartialEq)]
pub struct MtlxOptions {
    /// Surface models the host accepts; a material of another model is `NoTranslation`.
    pub models: Vec<String>,
    pub working_colorspace: String,
    /// Authored surface values and top-level interface values become uniforms.
    pub values_as_uniforms: bool,
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
/// `file` names the document (for messages and as the base of relative includes).
pub fn to_ir(
    text: &str,
    file: Option<&Path>,
    material: Option<&str>,
    options: &MtlxOptions,
) -> Result<MaterialIr, Error> {
    let name = file.map(|f| f.display().to_string());
    let root = parse_str(text, name.as_deref())?;
    if file.is_none() {
        if let Some(include) = root.children.iter().find(|c| c.category == "xi:include") {
            let at = Location { file: None, line: include.line, path: String::new() };
            return Err(Error::Parse { at, message: "includes need a file path to resolve against".to_string() });
        }
    }
    let root = resolve_includes(root, file.and_then(Path::parent), name.as_deref())?;
    let document = build(&root, name.as_deref())?;
    let (major, minor) = document.version;
    let document = upgrade(document)?;

    let elaborate_options = ElaborateOptions {
        working_colorspace: options.working_colorspace.clone(),
        values_as_uniforms: options.values_as_uniforms,
    };
    let models: Vec<&str> = options.models.iter().map(String::as_str).collect();
    let dag = elaborate(&document, standard(), &elaborate_options, material, &models)?;
    let source = SourceInfo { source: "mtlx".to_string(), mtlx_version: format!("{major}.{minor}") };
    Ok(MaterialIr { dag, source })
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
        let path = Path::new(DEFAULT);
        let text = std::fs::read_to_string(path).unwrap();
        let ir = to_ir(&text, Some(path), None, &MtlxOptions::default()).unwrap();
        assert_eq!(ir.dag.surface.model, "standard_surface");
        assert_eq!(ir.source.mtlx_version, "1.39");
        assert!(ir.dag.dump().starts_with("model standard_surface\n"), "{}", ir.dag.dump());
    }

    #[test]
    fn includes_without_a_file_are_an_error() {
        let text = r#"<materialx version="1.39"><xi:include href="other.mtlx"/></materialx>"#;
        let err = to_ir(text, None, None, &MtlxOptions::default()).unwrap_err();
        assert!(err.to_string().contains("includes need a file path"), "{err}");
    }
}
