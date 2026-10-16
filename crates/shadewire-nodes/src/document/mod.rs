//! The typed MaterialX document.

pub mod build;
pub mod flatten;
pub mod geompropdef;
pub mod inherit;
pub mod node;
pub mod nodedef;
pub mod nodegraph;
pub mod port_type;
pub mod ui;
pub mod unitdef;

pub use geompropdef::GeomPropDef;
pub use node::{Input, Node};
pub use nodedef::{NodeDef, PortDef};
pub use nodegraph::{NodeGraph, Output};
pub use port_type::PortType;
pub use ui::UiAttrs;
pub use unitdef::UnitDef;

/// Where an element was authored: its line in the document's file, and the host's own path for it when a builder
/// set one (`source_path`, shown in messages in place of the file and line).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Site<'s> {
    pub line: u32,
    pub source: Option<&'s str>,
}

impl Site<'_> {
    /// No site: line 0, no source.
    pub const NONE: Site<'static> = Site { line: 0, source: None };
}

macro_rules! sited {
    ($($ty:ty),*) => {$(
        impl $ty {
            /// Where this element was authored.
            pub fn site(&self) -> Site<'_> {
                Site { line: self.line, source: self.source.as_deref() }
            }
        }
    )*};
}

sited!(Input, Node, NodeGraph, Output, PortDef);

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
    /// `<implementation nodedef="..." nodegraph="..."/>`: nodedefs implemented by a graph that names another
    /// nodedef (`ND_standard_surface_surfaceshader` by `NG_standard_surface_surfaceshader_100`), as
    /// `(nodedef, nodegraph)`.
    pub graph_implementations: Vec<(String, String)>,
}

/// A document with nothing in it, for lowering that starts from no document (`Elaborator::standalone`).
pub fn empty_document() -> &'static Document {
    static EMPTY: std::sync::OnceLock<Document> = std::sync::OnceLock::new();
    EMPTY.get_or_init(Document::empty)
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
            graph_implementations: Vec::new(),
        }
    }
}
