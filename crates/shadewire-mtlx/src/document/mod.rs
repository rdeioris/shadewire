//! The typed MaterialX document.

pub mod build;
pub mod geompropdef;
pub mod inherit;
pub mod node;
pub mod nodedef;
pub mod nodegraph;
pub mod port_type;
pub mod unitdef;

pub use geompropdef::GeomPropDef;
pub use node::{Input, Node};
pub use nodedef::{NodeDef, PortDef};
pub use nodegraph::{NodeGraph, Output};
pub use port_type::PortType;
pub use unitdef::UnitDef;

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub version: (u32, u32),
    pub file: Option<String>,
    pub nodedefs: Vec<NodeDef>,
    pub nodegraphs: Vec<NodeGraph>,
    pub nodes: Vec<Node>,
    pub outputs: Vec<Output>,
    pub unitdefs: Vec<UnitDef>,
    pub geompropdefs: Vec<GeomPropDef>,
}

impl Document {
    pub fn empty() -> Document {
        Document {
            version: (1, 39),
            file: None,
            nodedefs: Vec::new(),
            nodegraphs: Vec::new(),
            nodes: Vec::new(),
            outputs: Vec::new(),
            unitdefs: Vec::new(),
            geompropdefs: Vec::new(),
        }
    }
}
