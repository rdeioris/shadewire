//! Elaboration: a MaterialX document becomes a typed DAG. Library graphs are
//! inlined per instance, leaf nodedefs become ops, inputs follow connections,
//! interfaces and defaults.

pub mod colorspace;
pub mod defaults;
pub mod fold;
pub mod hextile;
pub mod image;
pub mod image_grad;
pub mod inputs;
pub mod instance;
pub mod instantiate;
pub mod leaf;
pub mod matrix;
pub mod ramp;
pub mod scope;
pub mod special;
pub mod units;
pub mod values;

#[cfg(test)]
mod compound_tests;
#[cfg(test)]
mod connection_tests;
#[cfg(test)]
mod convert_tests;
#[cfg(test)]
mod cycle_tests;
#[cfg(test)]
mod depth_tests;
#[cfg(test)]
mod geomprop_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod instantiate_tests;
#[cfg(test)]
mod limit_tests;
#[cfg(test)]
mod param_tests;
#[cfg(test)]
mod special_tests;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};

use shadewire::{Compound, Dag, Error, Location, NodeId, SurfaceField, Type, Value};

use crate::document::{Document, PortType, Site};
use crate::index::Index;
use crate::library::Library;
use crate::options::IrOptions;
use crate::surface::CLOSURE_MODEL;
use crate::surface::distill::DISTILL_MODEL;
use crate::surface::find::{SurfaceSource, pick_material};
use crate::synthetic::Synthetic;

pub use instantiate::Arg;
pub use scope::{Binding, Scope, TextValue};

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
    pub options: &'a IrOptions,
    pub dag: Dag,
    pub memo: HashMap<MemoKey, NodeId>,
    pub in_progress: HashSet<MemoKey>,
    /// Node outputs being followed right now, inside one another.
    pub depth: u32,
    pub uniform_names: HashSet<String>,
    /// The origins a standalone elaborator gave its synthetic nodes, so a reused one is told apart.
    pub(crate) synthetic_origins: HashSet<String>,
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
    /// The models the host accepts.
    pub accepted_models: Vec<String>,
    /// The host accepts the `surface` model: `surface` nodes compile to closure trees, and named models
    /// outside `accepted_models` open through their implementation graphs.
    pub accepts_closures: bool,
    /// `IrOptions::distill` with `open_pbr_surface` accepted: a closure tree comes out distilled to it.
    pub distills: bool,
    /// The nodes a front end made up (`Elaborator::standalone`); `None` when built from a document.
    pub synthetic: Option<&'a Synthetic>,
}

impl<'a> Elaborator<'a> {
    pub fn new(doc: &'a Document, lib: &'a Library, options: &'a IrOptions, model: &str) -> Elaborator<'a> {
        Elaborator {
            index: Index::new(doc, lib),
            options,
            dag: Dag::new(model),
            memo: HashMap::new(),
            in_progress: HashSet::new(),
            depth: 0,
            uniform_names: HashSet::new(),
            synthetic_origins: HashSet::new(),
            file: doc.file.clone(),
            next_scope_id: 0,
            user_scopes: HashMap::new(),
            inlining: HashSet::new(),
            following_outputs: HashSet::new(),
            geomprop_defaults: HashMap::new(),
            surface_sources: HashMap::new(),
            surface_field_memo: HashMap::new(),
            accepted_models: Vec::new(),
            accepts_closures: false,
            distills: false,
            synthetic: None,
        }
    }

    /// The models `options` accept: closure trees when `surface` is among them, or when they are distilled to an
    /// accepted `open_pbr_surface`.
    pub fn accept(&mut self, options: &IrOptions) {
        self.accepted_models = options.models.clone();
        self.distills = options.distill && options.models.iter().any(|m| m == DISTILL_MODEL);
        self.accepts_closures = self.distills || options.models.iter().any(|m| m == CLOSURE_MODEL);
    }

    /// A location in `scope`: its library file, or the elaborated document's; the element's `source_path` when a
    /// builder gave it one.
    pub fn at(&self, scope: &Scope<'a>, site: Site<'_>, path: String) -> Location {
        let file = scope.file.map(str::to_string).or_else(|| self.file.clone());
        Location { file, line: site.line, path, source: site.source.map(str::to_string) }
    }

    /// The value type of a port. Matrices and non-value ports are unsupported.
    pub fn value_type(&self, port_type: &PortType, at: &Location) -> Result<Type, Error> {
        match port_type {
            PortType::Value(ty) if !ty.is_matrix() => Ok(*ty),
            PortType::Value(ty) => Err(Error::UnsupportedType { at: at.clone(), ty: ty.mtlx_name().to_string() }),
            other => Err(Error::UnsupportedType { at: at.clone(), ty: format!("{other:?}") }),
        }
    }

    /// Records that `node` is output `output` of an instance of `nodedef` at `origin`; a second output of the same
    /// instance joins the compound recorded for its first.
    pub fn record_compound(
        &mut self,
        nodedef: &str,
        inputs: Vec<(String, NodeId)>,
        output: &str,
        node: NodeId,
        origin: &str,
    ) {
        if let Some(found) = self.dag.compounds.iter_mut().find(|c| c.origin == origin && c.nodedef == nodedef) {
            if !found.outputs.iter().any(|(name, _)| name == output) {
                found.outputs.push((output.to_string(), node));
            }
            return;
        }
        self.dag.compounds.push(Compound {
            nodedef: nodedef.to_string(),
            inputs,
            outputs: vec![(output.to_string(), node)],
            origin: origin.to_string(),
        });
    }

    /// `Error::Limit` once the DAG has grown past `options.max_nodes`.
    pub fn check_node_limit(&self) -> Result<(), Error> {
        match self.options.max_nodes {
            Some(max) if self.dag.nodes.len() > max => {
                Err(Error::Limit(format!("the material needs more than {max} nodes (max_nodes)")))
            }
            _ => Ok(()),
        }
    }
}

/// Compiles the document's material into a DAG for one of `options.models`.
/// Elaboration recurses along the graph, so it runs on a thread with a large
/// stack; where no thread can be made it runs on the caller's.
pub fn elaborate(doc: &Document, lib: &Library, options: &IrOptions, material: Option<&str>) -> Result<Dag, Error> {
    std::thread::scope(|threads| {
        let builder = std::thread::Builder::new().name("shadewire-elaborate".to_string()).stack_size(STACK_BYTES);
        match builder.spawn_scoped(threads, || elaborate_here(doc, lib, options, material)) {
            Ok(handle) => match handle.join() {
                Ok(result) => result,
                Err(panic) => std::panic::resume_unwind(panic),
            },
            Err(_) => elaborate_here(doc, lib, options, material),
        }
    })
}

fn elaborate_here(doc: &Document, lib: &Library, options: &IrOptions, material: Option<&str>) -> Result<Dag, Error> {
    let mut elaborator = Elaborator::new(doc, lib, options, "");
    elaborator.accept(options);
    let material = pick_material(doc, material)?;
    let top = elaborator.top_scope(&doc.nodes);
    let source = elaborator.find_surface(&top, material)?;
    elaborator.finish_source(source)
}

/// Test helper: a DAG whose single surface field `out` is output `output` of user nodegraph `graph`.
#[cfg(test)]
pub(crate) fn elaborate_output(doc: &Document, graph: &str, output: &str) -> Result<Dag, Error> {
    elaborate_output_with(doc, graph, output, &IrOptions::default())
}

/// Test helper: `elaborate_output` under `options`.
#[cfg(test)]
pub(crate) fn elaborate_output_with(
    doc: &Document,
    graph: &str,
    output: &str,
    options: &IrOptions,
) -> Result<Dag, Error> {
    let mut elaborator = Elaborator::new(doc, crate::library::standard(), options, "test");
    let top = elaborator.top_scope(&doc.nodes);
    let at = elaborator.at(&top, Site::NONE, graph.to_string());
    let scope = elaborator.user_graph_scope(graph, &at)?;
    let id = elaborator.graph_output(&scope, Some(output), &at)?;
    let ty = elaborator.dag.node(id).ty;
    elaborator.dag.surface.fields.push(shadewire::SurfaceField { name: "out".to_string(), ty, node: id });
    Ok(elaborator.dag)
}
