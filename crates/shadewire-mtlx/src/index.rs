//! Name lookups over a user document and the standard library.

use crate::document::{Document, GeomPropDef, NodeDef, NodeGraph};
use crate::library::Library;

/// Documents are searched in order: the user's first, then the library's.
pub struct Index<'a> {
    documents: Vec<&'a Document>,
}

impl<'a> Index<'a> {
    pub fn new(user: &'a Document, lib: &'a Library) -> Index<'a> {
        let mut documents = vec![user];
        documents.extend(lib.documents.iter());
        Index { documents }
    }

    pub fn nodedef(&self, name: &str) -> Option<&'a NodeDef> {
        self.documents.iter().find_map(|d| d.nodedefs.iter().find(|n| n.name == name))
    }

    pub fn nodedefs_for_category(&self, category: &str) -> Vec<&'a NodeDef> {
        self.documents
            .iter()
            .flat_map(|d| d.nodedefs.iter())
            .filter(|n| n.node == category)
            .collect()
    }

    /// The nodegraph that implements a nodedef, if any.
    pub fn graph_for_nodedef(&self, nodedef: &str) -> Option<&'a NodeGraph> {
        self.documents
            .iter()
            .find_map(|d| d.nodegraphs.iter().find(|g| g.nodedef.as_deref() == Some(nodedef)))
    }

    pub fn nodegraph(&self, name: &str) -> Option<&'a NodeGraph> {
        self.documents.iter().find_map(|d| d.nodegraphs.iter().find(|g| g.name == name))
    }

    pub fn geompropdef(&self, name: &str) -> Option<&'a GeomPropDef> {
        self.documents.iter().find_map(|d| d.geompropdefs.iter().find(|g| g.name == name))
    }

    pub fn unit_scale(&self, unittype: &str, unit: &str) -> Option<f32> {
        self.documents
            .iter()
            .flat_map(|d| d.unitdefs.iter())
            .filter(|u| u.unittype == unittype)
            .find_map(|u| u.scales.iter().find(|(n, _)| n == unit).map(|(_, s)| *s))
    }

    /// A leaf nodedef has no nodegraph implementing it; the compiler maps it to an op.
    pub fn is_leaf(&self, nodedef: &NodeDef) -> bool {
        self.graph_for_nodedef(&nodedef.name).is_none()
    }
}
