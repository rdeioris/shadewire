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

    /// Matching nodedefs; a nodedef hides any later one with the same name.
    pub fn nodedefs_for_category(&self, category: &str) -> Vec<&'a NodeDef> {
        let mut found: Vec<&'a NodeDef> = Vec::new();
        for nodedef in self.documents.iter().flat_map(|d| d.nodedefs.iter()) {
            if nodedef.node == category && !found.iter().any(|f| f.name == nodedef.name) {
                found.push(nodedef);
            }
        }
        found
    }

    /// The nodegraph that implements a nodedef, if any.
    pub fn graph_for_nodedef(&self, nodedef: &str) -> Option<&'a NodeGraph> {
        let direct =
            self.documents.iter().find_map(|d| d.nodegraphs.iter().find(|g| g.nodedef.as_deref() == Some(nodedef)));
        direct.or_else(|| {
            let graph = self.documents.iter().find_map(|d| {
                d.graph_implementations.iter().find(|(nd, _)| nd == nodedef).map(|(_, graph)| graph.as_str())
            })?;
            self.nodegraph(graph)
        })
    }

    /// The file of the document holding `graph`, if it was named.
    pub fn graph_file(&self, graph: &NodeGraph) -> Option<&'a str> {
        let document = self.documents.iter().find(|d| d.nodegraphs.iter().any(|g| std::ptr::eq(g, graph)))?;
        document.file.as_deref()
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
