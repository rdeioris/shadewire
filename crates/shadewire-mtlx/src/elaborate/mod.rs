//! Elaboration: a MaterialX document becomes a typed DAG. Library graphs are
//! inlined per instance, leaf nodedefs become ops, inputs follow connections,
//! interfaces and defaults.

pub mod defaults;
pub mod image;
pub mod inputs;
pub mod instance;
pub mod leaf;
pub mod scope;
pub mod special;
pub mod values;

#[cfg(test)]
mod cycle_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod special_tests;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use shadewire_ir::{Dag, Error, Location, NodeId, Type, Value};

use crate::document::{Document, PortType};
use crate::index::Index;
use crate::library::Library;

pub use scope::{Binding, Scope, TextValue};

#[derive(Clone, Debug, PartialEq)]
pub struct ElaborateOptions {
    /// The colorspace authored colors are converted to.
    pub working_colorspace: String,
    /// Surface-node values and top-level nodegraph interface values become uniforms.
    pub values_as_uniforms: bool,
}

impl Default for ElaborateOptions {
    fn default() -> ElaborateOptions {
        ElaborateOptions { working_colorspace: "lin_rec709".to_string(), values_as_uniforms: true }
    }
}

/// `(scope id, node name, output name)`; the output name is `""` when not given.
pub type MemoKey = (u32, String, String);

pub struct Elaborator<'a> {
    pub index: Index<'a>,
    pub options: &'a ElaborateOptions,
    pub dag: Dag,
    pub memo: HashMap<MemoKey, NodeId>,
    pub in_progress: HashSet<MemoKey>,
    pub uniform_names: HashSet<String>,
    pub file: Option<String>,
    pub next_scope_id: u32,
    /// User nodegraphs entered so far, by name.
    pub user_scopes: HashMap<String, Scope<'a>>,
    /// Nodedefs whose implementation graph is being inlined right now.
    pub inlining: HashSet<String>,
    /// Graph outputs `(scope id, output name)` being followed right now.
    pub following_outputs: HashSet<(u32, String)>,
    /// `geompropvalue` defaults by property name, for reflection.
    pub geomprop_defaults: HashMap<String, Value>,
}

impl<'a> Elaborator<'a> {
    pub fn new(doc: &'a Document, lib: &'a Library, options: &'a ElaborateOptions, model: &str) -> Elaborator<'a> {
        Elaborator {
            index: Index::new(doc, lib),
            options,
            dag: Dag::new(model),
            memo: HashMap::new(),
            in_progress: HashSet::new(),
            uniform_names: HashSet::new(),
            file: doc.file.clone(),
            next_scope_id: 0,
            user_scopes: HashMap::new(),
            inlining: HashSet::new(),
            following_outputs: HashSet::new(),
            geomprop_defaults: HashMap::new(),
        }
    }

    pub fn at(&self, line: u32, path: String) -> Location {
        Location { file: self.file.clone(), line, path }
    }

    /// The value type of a port. Matrices and non-value ports are unsupported.
    pub fn value_type(&self, port_type: &PortType, at: &Location) -> Result<Type, Error> {
        match port_type {
            PortType::Value(ty) if !ty.is_matrix() => Ok(*ty),
            PortType::Value(ty) => Err(Error::UnsupportedType { at: at.clone(), ty: ty.mtlx_name().to_string() }),
            other => Err(Error::UnsupportedType { at: at.clone(), ty: format!("{other:?}") }),
        }
    }
}

/// Compiles the document's material into a DAG for one of `models`.
pub fn elaborate(
    doc: &Document,
    lib: &Library,
    options: &ElaborateOptions,
    material: Option<&str>,
    models: &[&str],
) -> Result<Dag, Error> {
    let _elaborator = Elaborator::new(doc, lib, options, "");
    let _ = (material, models);
    unimplemented!("task 18")
}

/// Test helper: a DAG whose single surface field `out` is output `output` of user nodegraph `graph`.
#[cfg(test)]
pub(crate) fn elaborate_output(doc: &Document, graph: &str, output: &str) -> Result<Dag, Error> {
    let options = ElaborateOptions::default();
    let mut elaborator = Elaborator::new(doc, crate::library::standard(), &options, "test");
    let at = elaborator.at(0, graph.to_string());
    let scope = elaborator.user_graph_scope(graph, &at)?;
    let id = elaborator.graph_output(&scope, Some(output), &at)?;
    let ty = elaborator.dag.node(id).ty;
    elaborator.dag.surface.fields.push(shadewire_ir::SurfaceField { name: "out".to_string(), ty, node: id });
    Ok(elaborator.dag)
}
