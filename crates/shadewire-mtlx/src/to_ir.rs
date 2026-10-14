//! The MaterialX front end in one call: text to the IR.

use shadewire::{Dag, Error, SourceInfo};

use crate::include::{Resolver, resolve_includes};
use crate::upgrade::upgrade;
use shadewire_nodes::IrOptions;
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
    options: &IrOptions,
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
        let ir = to_ir(&text, Some(DEFAULT), None, &IrOptions::default(), &Resolver::default()).unwrap();
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
        let options = IrOptions::with_models(&["standard_surface"]);
        let err = to_ir(&text, None, None, &options, &Resolver::default()).unwrap_err();
        assert!(matches!(err, Error::NoTranslation { .. }), "{err}");
    }

    #[test]
    fn material_names_are_listed_in_document_order() {
        let text = r#"<materialx version="1.39">
          <surfacematerial name="b" type="material"/><surfacematerial name="a" type="material"/></materialx>"#;
        assert_eq!(material_names(text, None, &Resolver::default()).unwrap(), ["b", "a"]);
    }

    /// A porcelain glaze over a diffuse and subsurface mix, as 1.38 wrote it: `radius` of the subsurface BSDF is a
    /// vector3.
    #[test]
    fn a_1_38_subsurface_document_upgrades_into_a_closure_tree() {
        let text = r#"<materialx version="1.38">
          <nodegraph name="NG_porcelain">
            <subsurface_bsdf name="node1" type="BSDF">
              <input name="color" type="color3" value="0.75, 0.75, 0.75" />
            </subsurface_bsdf>
            <subsurface_bsdf name="node2" type="BSDF">
              <input name="radius" type="vector3" value="0.3, 0.3, 0.3" />
              <input name="color" type="color3" value="0.75, 0.75, 0.75" />
            </subsurface_bsdf>
            <mix name="node3" type="BSDF">
              <input name="bg" type="BSDF" nodename="node1" />
              <input name="fg" type="BSDF" nodename="node2" />
              <input name="mix" type="float" value="0.35" />
            </mix>
            <oren_nayar_diffuse_bsdf name="node4" type="BSDF">
              <input name="color" type="color3" value="0.9, 0.9, 0.9" />
              <input name="roughness" type="float" value="0.65" />
            </oren_nayar_diffuse_bsdf>
            <mix name="node5" type="BSDF">
              <input name="bg" type="BSDF" nodename="node4" />
              <input name="fg" type="BSDF" nodename="node3" />
              <input name="mix" type="float" value="0.35" />
            </mix>
            <dielectric_bsdf name="node6" type="BSDF">
              <input name="weight" type="float" value="0.35" />
            </dielectric_bsdf>
            <layer name="node7" type="BSDF">
              <input name="top" type="BSDF" nodename="node6" />
              <input name="base" type="BSDF" nodename="node5" />
            </layer>
            <output name="out_bsdf" type="BSDF" nodename="node7" />
          </nodegraph>
          <surface name="SR_porcelain" type="surfaceshader">
            <input name="bsdf" type="BSDF" nodegraph="NG_porcelain" output="out_bsdf" />
          </surface>
          <surfacematerial name="porcelain" type="material">
            <input name="surfaceshader" type="surfaceshader" nodename="SR_porcelain" />
          </surfacematerial>
        </materialx>"#;
        let ir = to_ir(text, None, None, &IrOptions::with_models(&["surface"]), &Resolver::default()).unwrap();
        assert_eq!(ir.dag.surface.model, "surface");
        let closure = ir.dag.surface.closure.as_ref().unwrap();
        let kinds: Vec<&str> = closure
            .nodes
            .iter()
            .filter_map(|n| match n {
                shadewire::ClosureNode::Lobe { kind, .. } => Some(kind.as_str()),
                _ => None,
            })
            .collect();
        assert!(kinds.contains(&"subsurface_bsdf"), "{kinds:?}");
    }

    #[test]
    fn includes_without_a_file_are_an_error() {
        let text = r#"<materialx version="1.39"><xi:include href="other.mtlx"/></materialx>"#;
        let err = to_ir(text, None, None, &IrOptions::default(), &Resolver::default()).unwrap_err();
        assert!(err.to_string().contains("includes need a document location"), "{err}");
    }
}
