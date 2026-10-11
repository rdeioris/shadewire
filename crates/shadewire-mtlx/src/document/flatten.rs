//! Nodedef inheritance (`inherit=`), flattened once when the library loads.

use super::{Document, NodeDef, PortDef};

/// Deepest inheritance chain accepted; deeper is a library defect.
const MAX_DEPTH: usize = 8;

/// Gives every nodedef with `inherit=` its ancestors' ports, the way
/// MaterialX `getActiveInputs` / `getActiveOutputs` list them: the nodedef's
/// own ports first, then each ancestor's ports whose names are not already
/// present. An own port overrides the inherited one of the same name.
/// A missing parent, a cycle or a chain deeper than 8 is an error naming the nodedef.
pub fn flatten_inheritance(documents: &mut [Document]) -> Result<(), String> {
    let snapshot: Vec<NodeDef> = documents.iter().flat_map(|d| d.nodedefs.iter().cloned()).collect();
    for doc in documents.iter_mut() {
        for nodedef in &mut doc.nodedefs {
            if nodedef.inherits.is_none() {
                continue;
            }
            let (inputs, outputs) = active_ports(&snapshot, nodedef, &mut Vec::new())?;
            nodedef.inputs = inputs;
            nodedef.outputs = outputs;
            nodedef.inherits = None;
        }
    }
    Ok(())
}

/// `(inputs, outputs)` of `nodedef` with its whole chain folded in. `chain`
/// holds the names already visited on this path.
fn active_ports(all: &[NodeDef], nodedef: &NodeDef, chain: &mut Vec<String>) -> Result<(Vec<PortDef>, Vec<PortDef>), String> {
    if chain.contains(&nodedef.name) {
        return Err(format!("nodedef {} inherits from itself through {}", nodedef.name, chain.join(" -> ")));
    }
    if chain.len() > MAX_DEPTH {
        return Err(format!("nodedef {} has an inheritance chain deeper than {MAX_DEPTH}", chain[0]));
    }
    let Some(parent_name) = &nodedef.inherits else {
        return Ok((nodedef.inputs.clone(), nodedef.outputs.clone()));
    };
    let parent = all
        .iter()
        .find(|n| &n.name == parent_name)
        .ok_or_else(|| format!("nodedef {} inherits from missing nodedef {parent_name}", nodedef.name))?;
    chain.push(nodedef.name.clone());
    let (parent_inputs, parent_outputs) = active_ports(all, parent, chain)?;
    Ok((merged(&nodedef.inputs, parent_inputs), merged(&nodedef.outputs, parent_outputs)))
}

/// `own`, then the inherited ports whose names `own` does not use.
fn merged(own: &[PortDef], inherited: Vec<PortDef>) -> Vec<PortDef> {
    let mut out = own.to_vec();
    out.extend(inherited.into_iter().filter(|p| !own.iter().any(|o| o.name == p.name)));
    out
}

#[cfg(test)]
mod tests {
    use super::flatten_inheritance;
    use crate::document::build::build;
    use crate::document::Document;
    use crate::parse::parse_str;

    fn docs(body: &str) -> Vec<Document> {
        let text = format!(r#"<materialx version="1.39">{body}</materialx>"#);
        vec![build(&parse_str(&text, None).unwrap(), None).unwrap()]
    }

    fn names(ports: &[crate::document::PortDef]) -> Vec<&str> {
        ports.iter().map(|p| p.name.as_str()).collect()
    }

    #[test]
    fn own_inputs_first_then_the_parents_like_get_active_inputs() {
        let mut d = docs(
            r#"<nodedef name="ND_child" node="x" inherit="ND_mid">
                 <input name="c" type="float" value="3"/><input name="a" type="float" value="9"/>
                 <output name="extra" type="float"/>
               </nodedef>
               <nodedef name="ND_mid" node="x" inherit="ND_base"><input name="m" type="float"/></nodedef>
               <nodedef name="ND_base" node="x">
                 <input name="a" type="float" value="1"/><input name="b" type="float"/>
                 <output name="out" type="float"/><output name="extra" type="float"/>
               </nodedef>"#,
        );
        flatten_inheritance(&mut d).unwrap();
        let child = &d[0].nodedefs[0];
        assert_eq!(names(&child.inputs), ["c", "a", "m", "b"]);
        assert_eq!(child.inputs[1].value.as_deref(), Some("9"));
        assert_eq!(names(&child.outputs), ["extra", "out"]);
        assert_eq!(child.inherits, None);
    }

    #[test]
    fn a_missing_parent_is_an_error_naming_the_nodedef() {
        let mut d = docs(r#"<nodedef name="ND_orphan" node="x" inherit="ND_gone"/>"#);
        let err = flatten_inheritance(&mut d).unwrap_err();
        assert!(err.contains("ND_orphan") && err.contains("ND_gone"), "{err}");
    }

    #[test]
    fn an_inheritance_cycle_is_an_error() {
        let mut d = docs(r#"<nodedef name="ND_a" node="x" inherit="ND_b"/><nodedef name="ND_b" node="x" inherit="ND_a"/>"#);
        let err = flatten_inheritance(&mut d).unwrap_err();
        assert!(err.contains("ND_a") || err.contains("ND_b"), "{err}");
    }
}
