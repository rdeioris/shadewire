//! Elaboration: a MaterialX document becomes a typed DAG. Library graphs are
//! inlined per instance, leaf nodedefs become ops, inputs follow connections,
//! interfaces and defaults.

pub mod colorspace;
pub mod defaults;
pub mod fold;
pub mod hextile;
pub mod image;
pub mod inputs;
pub mod instance;
pub mod leaf;
pub mod matrix;
pub mod ramp;
pub mod scope;
pub mod special;
pub mod units;
pub mod values;

#[cfg(test)]
mod connection_tests;
#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod cycle_tests;
#[cfg(test)]
mod depth_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod param_tests;
#[cfg(test)]
mod special_tests;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use shadewire::{Dag, Error, Location, NodeId, SurfaceField, Type, Value};

use crate::document::{Document, PortType};
use crate::index::Index;
use crate::library::Library;
use crate::surface::find::{SurfaceSource, pick_material};

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

/// How many node outputs elaboration follows inside one another before it
/// gives up. Real graphs are under 100 deep; a debug build uses about 12 KB of
/// stack per level, so `elaborate` runs on a thread with `STACK_BYTES` of stack.
pub const MAX_GRAPH_DEPTH: u32 = 1024;

/// The stack of the elaboration thread: room for `MAX_GRAPH_DEPTH` levels in a debug build.
pub const STACK_BYTES: usize = 64 * 1024 * 1024;

/// `(scope id, node name, output name)`; the output name is `""` when not given.
pub type MemoKey = (u32, String, String);

pub struct Elaborator<'a> {
    pub index: Index<'a>,
    pub options: &'a ElaborateOptions,
    pub dag: Dag,
    pub memo: HashMap<MemoKey, NodeId>,
    pub in_progress: HashSet<MemoKey>,
    /// Node outputs being followed right now, inside one another.
    pub depth: u32,
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
    /// Surface sources found so far, by `(scope id, shader node name)`.
    pub surface_sources: HashMap<(u32, String), SurfaceSource<'a>>,
    /// Surface fields elaborated so far, by `SurfaceSource::key`.
    pub surface_field_memo: HashMap<(u32, String), Vec<SurfaceField>>,
}

impl<'a> Elaborator<'a> {
    pub fn new(doc: &'a Document, lib: &'a Library, options: &'a ElaborateOptions, model: &str) -> Elaborator<'a> {
        Elaborator {
            index: Index::new(doc, lib),
            options,
            dag: Dag::new(model),
            memo: HashMap::new(),
            in_progress: HashSet::new(),
            depth: 0,
            uniform_names: HashSet::new(),
            file: doc.file.clone(),
            next_scope_id: 0,
            user_scopes: HashMap::new(),
            inlining: HashSet::new(),
            following_outputs: HashSet::new(),
            geomprop_defaults: HashMap::new(),
            surface_sources: HashMap::new(),
            surface_field_memo: HashMap::new(),
        }
    }

    /// A location in `scope`: its library file, or the elaborated document's.
    pub fn at(&self, scope: &Scope<'a>, line: u32, path: String) -> Location {
        let file = scope.file.map(str::to_string).or_else(|| self.file.clone());
        Location { file, line, path }
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
/// Elaboration recurses along the graph, so it runs on a thread with a large
/// stack; where no thread can be made it runs on the caller's.
pub fn elaborate(
    doc: &Document,
    lib: &Library,
    options: &ElaborateOptions,
    material: Option<&str>,
    models: &[&str],
) -> Result<Dag, Error> {
    std::thread::scope(|threads| {
        let builder = std::thread::Builder::new().name("shadewire-elaborate".to_string()).stack_size(STACK_BYTES);
        match builder.spawn_scoped(threads, || elaborate_here(doc, lib, options, material, models)) {
            Ok(handle) => match handle.join() {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => elaborate_here(doc, lib, options, material, models),
        }
    })
}

fn elaborate_here(
    doc: &Document,
    lib: &Library,
    options: &ElaborateOptions,
    material: Option<&str>,
    models: &[&str],
) -> Result<Dag, Error> {
    let mut elaborator = Elaborator::new(doc, lib, options, "");
    let material = pick_material(doc, material)?;
    let top = elaborator.top_scope(&doc.nodes);
    let source = elaborator.find_surface(&top, material)?;
    let model = source.model().to_string();
    if !models.contains(&model.as_str()) {
        return Err(Error::NoTranslation { from: model, to: models.iter().map(|m| m.to_string()).collect() });
    }
    let fields = elaborator.surface_fields(&source)?;
    elaborator.dag.surface.fields = fields;
    elaborator.dag.surface.model = model;
    elaborator.dag.geomprop_defaults = elaborator.geomprop_defaults.drain().collect();
    Ok(elaborator.dag)
}

/// Test helper: a DAG whose single surface field `out` is output `output` of user nodegraph `graph`.
#[cfg(test)]
pub(crate) fn elaborate_output(doc: &Document, graph: &str, output: &str) -> Result<Dag, Error> {
    let options = ElaborateOptions::default();
    let mut elaborator = Elaborator::new(doc, crate::library::standard(), &options, "test");
    let top = elaborator.top_scope(&doc.nodes);
    let at = elaborator.at(&top, 0, graph.to_string());
    let scope = elaborator.user_graph_scope(graph, &at)?;
    let id = elaborator.graph_output(&scope, Some(output), &at)?;
    let ty = elaborator.dag.node(id).ty;
    elaborator.dag.surface.fields.push(shadewire::SurfaceField { name: "out".to_string(), ty, node: id });
    Ok(elaborator.dag)
}
