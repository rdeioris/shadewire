//! A mix whose sides share a sub-mix elaborates that sub-mix once.

use shadewire::{Dag, NodeKind};

use crate::document::build::build;
use crate::library::standard;
use crate::parse::parse_str;
use crate::{LowerOptions, elaborate};

/// `levels` surface mixes, each with fg = bg = the one before, over one unlit surface.
fn diamond_chain(levels: usize) -> Dag {
    let mut body = String::from(
        r#"<surface_unlit name="m0" type="surfaceshader"><input name="emission" type="float" value="0.5"/></surface_unlit>"#,
    );
    for i in 1..=levels {
        let previous = i - 1;
        body.push_str(&format!(
            r#"<mix name="m{i}" type="surfaceshader">
              <input name="fg" type="surfaceshader" nodename="m{previous}"/>
              <input name="bg" type="surfaceshader" nodename="m{previous}"/>
              <input name="mix" type="float" value="0.5"/>
            </mix>"#
        ));
    }
    body.push_str(&format!(
        r#"<surfacematerial name="mat" type="material"><input name="surfaceshader" type="surfaceshader" nodename="m{levels}"/></surfacematerial>"#
    ));
    let text = format!(r#"<materialx version="1.39">{body}</materialx>"#);
    let doc = build(&parse_str(&text, None).unwrap(), Some("test.mtlx")).unwrap();
    elaborate(&doc, standard(), &LowerOptions::with_models(&["surface_unlit"]), None).unwrap()
}

#[test]
fn shared_sub_mixes_are_elaborated_once() {
    let dag = diamond_chain(20);
    let fields = dag.surface.fields.len();
    // Each level adds one weight uniform and one blend per field.
    assert!(dag.nodes.len() <= fields * 2 + 20 * (fields + 2), "{} nodes", dag.nodes.len());
    let emission = dag.nodes.iter().filter(|n| matches!(&n.kind, NodeKind::Uniform { name, .. } if name == "emission"));
    assert_eq!(emission.count(), 1);
}
