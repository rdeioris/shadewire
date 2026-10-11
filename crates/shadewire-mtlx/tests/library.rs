use shadewire_ir::{input_names, op_for_category};
use shadewire_mtlx::document::PortType;
use shadewire_mtlx::{library, Document, Index};

#[test]
fn library_has_the_phase_one_models() {
    let empty = Document::empty();
    let index = Index::new(&empty, library::standard());
    for model in ["open_pbr_surface", "standard_surface", "gltf_pbr", "surface_unlit"] {
        assert!(!index.nodedefs_for_category(model).is_empty(), "{model} missing");
    }
}

/// Every leaf nodedef we map to an op must have exactly the op's value inputs.
#[test]
fn op_inputs_match_library() {
    let empty = Document::empty();
    let index = Index::new(&empty, library::standard());
    let mut problems = Vec::new();
    for doc in &library::standard().documents {
        for nd in &doc.nodedefs {
            let Some(PortType::Value(out)) = nd.outputs.first().map(|o| o.port_type.clone()) else { continue };
            let value_inputs: Vec<&str> = nd.inputs.iter()
                .filter(|i| matches!(i.port_type, PortType::Value(_)))
                // `extract.index` is an integer op parameter handled in elaboration, not an arg.
                .filter(|i| !(nd.node == "extract" && i.name == "index"))
                .map(|i| i.name.as_str()).collect();
            let Some(op) = op_for_category(&nd.node, out, value_inputs.len()) else { continue };
            if !index.is_leaf(nd) { continue; }
            let expected = input_names(&op);
            for name in &value_inputs {
                if !expected.contains(name) { problems.push(format!("{}: input {name} not in {:?}", nd.name, op)); }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn index_lookups() {
    let empty = Document::empty();
    let index = Index::new(&empty, library::standard());
    assert!(index.nodedef("ND_add_float").is_some());
    assert!(index.unit_scale("distance", "meter").is_some());
    assert!(index.geompropdef("Nworld").is_some());
    assert!(index.nodedef("ND_nope").is_none());
}
