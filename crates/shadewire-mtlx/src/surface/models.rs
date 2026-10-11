//! The shading models phase 1 compiles, and the fields of their surface structs.

use shadewire_ir::Type;

use crate::document::{NodeDef, PortType};

/// Shading models a surface can be compiled as without translation.
pub const PHASE_ONE_MODELS: [&str; 4] = ["open_pbr_surface", "standard_surface", "gltf_pbr", "surface_unlit"];

/// The WGSL struct name of a model; the one rule lives in the IR crate.
pub use shadewire_ir::model_struct_name as struct_name;

/// Every value-typed input of the model's nodedef, in nodedef order.
/// String and filename inputs are not fields.
pub fn fields(nd: &NodeDef) -> Vec<(String, Type)> {
    nd.inputs
        .iter()
        .filter_map(|port| match port.port_type {
            PortType::Value(ty) => Some((port.name.clone(), ty)),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Index;
    use crate::library::standard;
    use crate::Document;

    #[test]
    fn struct_name_is_the_ir_rule() {
        assert_eq!(struct_name("open_pbr_surface"), "OpenPbrSurface");
        assert_eq!(struct_name("gltf_pbr"), "GltfPbr");
    }

    #[test]
    fn fields_are_value_inputs_in_nodedef_order() {
        let doc = Document::empty();
        let index = Index::new(&doc, standard());
        let nd = index.nodedef("ND_open_pbr_surface_surfaceshader").unwrap();
        let fields = fields(nd);
        assert_eq!(fields[0], ("base_weight".to_string(), Type::Float));
        assert_eq!(fields[1], ("base_color".to_string(), Type::Color3));
        let values = nd.inputs.iter().filter(|p| matches!(p.port_type, PortType::Value(_))).count();
        assert_eq!(fields.len(), values);
    }

    #[test]
    fn string_and_filename_inputs_are_not_fields() {
        let text = r#"<materialx version="1.39">
          <nodedef name="ND_toy" node="toy">
            <input name="a" type="float" value="0"/>
            <input name="mode" type="string" value="x"/>
            <input name="file" type="filename" value=""/>
            <input name="b" type="vector3" value="0, 0, 0"/>
            <output name="out" type="surfaceshader"/>
          </nodedef></materialx>"#;
        let doc = crate::document::build::build(&crate::parse::parse_str(text, None).unwrap(), None).unwrap();
        let names: Vec<String> = fields(&doc.nodedefs[0]).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["a", "b"]);
    }
}
