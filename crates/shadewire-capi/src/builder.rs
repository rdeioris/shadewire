//! The graph builder (`shadewire_nodes::DocumentBuilder`): a MaterialX document built in memory, call by call, as
//! the opaque `shadewire_builder` handle, with its nodegraphs and nodes as `shadewire_nodegraph` and
//! `shadewire_node` handles it owns.
//!
//! Handles are boxed records kept in maps of their builder, keyed by address. A call looks a handle up by its
//! address in the builder it is given and never dereferences it, so a handle of another builder or a stray pointer
//! is refused with an error, not read. The one case left to the caller: once a builder is freed, the address of one of
//! its handles may be reused by a handle of a later builder, and then names that builder's element.

use std::collections::HashMap;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;

use shadewire_nodes::DocumentBuilder;
use shadewire_nodes::document::builder::{GraphRef, InputValue, NodeRef, Source};

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::shadewire_ir;
use crate::options::{read_mtlx, shadewire_mtlx_options};
use crate::status::shadewire_status;
use crate::strings::{ArgError, opt_cstr};

/// A MaterialX document under construction. Make it with `shadewire_builder_new`, fill it with the
/// `shadewire_builder_*`, `shadewire_nodegraph_*` and `shadewire_node_*` functions, lower it with
/// `shadewire_builder_to_ir` (as often as needed), and free it with `shadewire_builder_free`.
///
/// Each element is checked as it is added, so a mistake fails the call that made it; a failed call leaves the
/// builder as it was. Every element may carry the host's own path for it (`source_path`, NULL for none), which
/// messages show in place of a file and line.
///
/// A builder is not thread-safe: calls on one builder (including `shadewire_builder_to_ir`) must not overlap. It
/// may be used from any thread, one call at a time; separate builders are independent.
pub struct shadewire_builder {
    inner: DocumentBuilder,
    graphs: HashMap<usize, Box<shadewire_nodegraph>>,
    nodes: HashMap<usize, Box<shadewire_node>>,
}

/// A nodegraph of a builder, owned by it: valid until `shadewire_builder_free`, never freed by the caller. Pass it
/// only with its own builder: another builder refuses it (it is looked up by address, not read), but after its
/// builder is freed its address may name an element of a newer builder.
pub struct shadewire_nodegraph {
    graph: GraphRef,
}

/// A node of a builder (in the document or in a nodegraph), owned by it: valid until `shadewire_builder_free`, never
/// freed by the caller. Pass it only with its own builder, as for `shadewire_nodegraph`.
pub struct shadewire_node {
    node: NodeRef,
}

/// Which field of a `shadewire_value` holds the value. Passed in from C, so a plain `u32`.
pub type shadewire_value_kind = u32;
/// `floats` holds `count` floats: 1 for a float, 3 for a color3, and so on for every float-based type.
pub const SHADEWIRE_VALUE_FLOATS: shadewire_value_kind = 0;
/// `integer`, for an integer input.
pub const SHADEWIRE_VALUE_INTEGER: shadewire_value_kind = 1;
/// `boolean`, for a boolean input.
pub const SHADEWIRE_VALUE_BOOLEAN: shadewire_value_kind = 2;
/// `string` (NUL-terminated UTF-8), for a string or filename input.
pub const SHADEWIRE_VALUE_STRING: shadewire_value_kind = 3;

/// A typed value: `kind` says which field is read; the others are ignored. A kind that does not fit the input's type
/// is an error, and so is a float count that does not match it or a non-finite float. `floats` may be NULL only when
/// `count` is 0; `string` must not be NULL for SHADEWIRE_VALUE_STRING. `boolean` is true for any non-zero byte.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_value {
    pub kind: shadewire_value_kind,
    pub floats: *const f32,
    pub count: usize,
    pub integer: i32,
    pub boolean: bool,
    pub string: *const c_char,
}

/// Which element a `shadewire_source` reads. Passed in from C, so a plain `u32`.
pub type shadewire_source_kind = u32;
/// `node`, a node of the same scope, by `output` when it has several (NULL otherwise).
pub const SHADEWIRE_SOURCE_NODE: shadewire_source_kind = 0;
/// Output `output` of `nodegraph`.
pub const SHADEWIRE_SOURCE_NODEGRAPH: shadewire_source_kind = 1;
/// The interface input `interface_name` of the enclosing nodegraph.
pub const SHADEWIRE_SOURCE_INTERFACE: shadewire_source_kind = 2;

/// What an input or an output reads; `kind` says which fields are read, the others are ignored.
///
/// SHADEWIRE_SOURCE_NODE needs `node` (`output` may be NULL); SHADEWIRE_SOURCE_NODEGRAPH needs `nodegraph` and
/// `output`; SHADEWIRE_SOURCE_INTERFACE needs `interface_name`. A NULL where one is needed, a handle of another
/// builder, or an unknown kind is an error.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct shadewire_source {
    pub kind: shadewire_source_kind,
    pub node: *const shadewire_node,
    pub nodegraph: *const shadewire_nodegraph,
    pub output: *const c_char,
    pub interface_name: *const c_char,
}

impl shadewire_builder {
    /// The nodegraph `ptr` names in this builder, found by address: `ptr` is not dereferenced.
    fn graph(&self, ptr: *const shadewire_nodegraph) -> Result<GraphRef, ArgError> {
        if ptr.is_null() {
            return Err(ArgError("nodegraph is NULL".to_string()));
        }
        match self.graphs.get(&(ptr as usize)) {
            Some(record) => Ok(record.graph),
            None => Err(ArgError("not a nodegraph of this builder".to_string())),
        }
    }

    /// The nodegraph `ptr` names, `None` for NULL (the document).
    fn scope(&self, ptr: *const shadewire_nodegraph) -> Result<Option<GraphRef>, ArgError> {
        if ptr.is_null() { Ok(None) } else { self.graph(ptr).map(Some) }
    }

    /// The node `ptr` names in this builder, found by address: `ptr` is not dereferenced.
    fn node(&self, ptr: *const shadewire_node) -> Result<NodeRef, ArgError> {
        if ptr.is_null() {
            return Err(ArgError("node is NULL".to_string()));
        }
        match self.nodes.get(&(ptr as usize)) {
            Some(record) => Ok(record.node),
            None => Err(ArgError("not a node of this builder".to_string())),
        }
    }

    /// Keeps a record of `node` and returns its handle.
    fn keep_node(&mut self, node: NodeRef) -> *mut shadewire_node {
        let mut record = Box::new(shadewire_node { node });
        let ptr: *mut shadewire_node = &mut *record;
        self.nodes.insert(ptr as usize, record);
        ptr
    }
}

/// Borrows the builder behind `ptr`; NULL is an error.
///
/// # Safety
/// `ptr` must be NULL or a live builder that outlives `'a`, not used by another call meanwhile.
unsafe fn builder_mut<'a>(ptr: *mut shadewire_builder) -> Result<&'a mut shadewire_builder, ArgError> {
    unsafe { ptr.as_mut() }.ok_or_else(|| ArgError("builder is NULL".to_string()))
}

/// Reads a NUL-terminated UTF-8 argument that must be present.
///
/// # Safety
/// `ptr` must be NULL or a valid NUL-terminated string that outlives `'a`.
unsafe fn required<'a>(ptr: *const c_char, what: &str) -> Result<&'a str, ArgError> {
    unsafe { opt_cstr(ptr) }?.ok_or_else(|| ArgError(format!("{what} is NULL")))
}

/// Reads a value; NULL is an error. Fields are read one by one, `boolean` as its byte, so any byte a C `bool` holds
/// is fine.
///
/// # Safety
/// `ptr` must be NULL or point to a readable `shadewire_value` whose pointers are valid as its doc comment says and
/// outlive `'a`.
unsafe fn read_value<'a>(ptr: *const shadewire_value) -> Result<InputValue<'a>, ArgError> {
    if ptr.is_null() {
        return Err(ArgError("value is NULL".to_string()));
    }
    unsafe {
        match (&raw const (*ptr).kind).read() {
            SHADEWIRE_VALUE_FLOATS => {
                let floats = (&raw const (*ptr).floats).read();
                let count = (&raw const (*ptr).count).read();
                if count == 0 {
                    return Ok(InputValue::Floats(&[]));
                }
                if floats.is_null() {
                    return Err(ArgError(format!("floats is NULL but count is {count}")));
                }
                Ok(InputValue::Floats(std::slice::from_raw_parts(floats, count)))
            }
            SHADEWIRE_VALUE_INTEGER => Ok(InputValue::Integer((&raw const (*ptr).integer).read())),
            SHADEWIRE_VALUE_BOOLEAN => Ok(InputValue::Boolean((&raw const (*ptr).boolean).cast::<u8>().read() != 0)),
            SHADEWIRE_VALUE_STRING => Ok(InputValue::Text(required((&raw const (*ptr).string).read(), "string")?)),
            other => Err(ArgError(format!("unknown value kind {other}"))),
        }
    }
}

/// Reads a source against `builder`; NULL is an error.
///
/// # Safety
/// `ptr` must be NULL or point to a readable `shadewire_source` whose strings are NULL or valid NUL-terminated
/// strings that outlive `'a`.
unsafe fn read_source<'a>(builder: &shadewire_builder, ptr: *const shadewire_source) -> Result<Source<'a>, ArgError> {
    let Some(source) = (unsafe { ptr.as_ref() }) else {
        return Err(ArgError("from is NULL".to_string()));
    };
    unsafe {
        match source.kind {
            SHADEWIRE_SOURCE_NODE => Ok(Source::Node(builder.node(source.node)?, opt_cstr(source.output)?)),
            SHADEWIRE_SOURCE_NODEGRAPH => {
                Ok(Source::Graph(builder.graph(source.nodegraph)?, required(source.output, "output")?))
            }
            SHADEWIRE_SOURCE_INTERFACE => Ok(Source::Interface(required(source.interface_name, "interface_name")?)),
            other => Err(ArgError(format!("unknown source kind {other}"))),
        }
    }
}

/// A new, empty MaterialX 1.39 document whose nodedefs are looked up in the standard library; NULL only when
/// allocation fails. Free it with `shadewire_builder_free`.
#[unsafe(no_mangle)]
pub extern "C" fn shadewire_builder_new() -> *mut shadewire_builder {
    let made = std::panic::catch_unwind(|| {
        Box::into_raw(Box::new(shadewire_builder {
            inner: DocumentBuilder::new(),
            graphs: HashMap::new(),
            nodes: HashMap::new(),
        }))
    });
    made.unwrap_or(std::ptr::null_mut())
}

/// Frees `b` and every nodegraph and node handle it gave out; NULL is allowed. IRs made from it stay valid.
///
/// # Safety
/// `b` must be NULL or a builder from `shadewire_builder_new` not yet freed, with no call on it running.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_builder_free(b: *mut shadewire_builder) {
    if !b.is_null() {
        drop(unsafe { Box::from_raw(b) });
    }
}

/// Adds a nodegraph named `name` to the document and writes its handle to `out` (written only on success). The name
/// is unique among the document's nodes, nodegraphs and outputs.
///
/// # Safety
/// `b` must be NULL or a live builder; `name` and `source_path` NULL or valid NUL-terminated strings; `out` valid for
/// writing a pointer; `error` NULL or valid for writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_builder_add_nodegraph(
    b: *mut shadewire_builder,
    name: *const c_char,
    source_path: *const c_char,
    out: *mut *mut shadewire_nodegraph,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let b = builder_mut(b)?;
                let name = required(name, "name")?;
                let graph = b.inner.add_nodegraph(name, opt_cstr(source_path)?)?;
                let mut record = Box::new(shadewire_nodegraph { graph });
                let ptr: *mut shadewire_nodegraph = &mut *record;
                b.graphs.insert(ptr as usize, record);
                *out = ptr;
                Ok(())
            }),
        )
    }
}

/// Adds an interface input `name` of MaterialX type `type` to `ng`, with `value` as its default (NULL for none).
///
/// # Safety
/// `b` must be NULL or a live builder; `ng` NULL or any pointer (it is looked up, not read); `name`, `type` and
/// `source_path` NULL or valid NUL-terminated strings; `value` NULL or valid (see `shadewire_value`); `error` NULL or
/// valid for writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_nodegraph_add_input(
    b: *mut shadewire_builder,
    ng: *const shadewire_nodegraph,
    name: *const c_char,
    r#type: *const c_char,
    value: *const shadewire_value,
    source_path: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let graph = b.graph(ng)?;
                let name = required(name, "name")?;
                let ty = required(r#type, "type")?;
                let value = if value.is_null() { None } else { Some(read_value(value)?) };
                b.inner.add_interface_input(graph, name, ty, value, opt_cstr(source_path)?)?;
                Ok(())
            }),
        )
    }
}

/// Sets the colorspace of the interface input `name` of `ng`, already added. A type without one (a float) drops it,
/// as the XML reader does.
///
/// # Safety
/// `b` must be NULL or a live builder; `ng` NULL or any pointer (it is looked up, not read); `name` and `colorspace`
/// NULL or valid NUL-terminated strings; `error` NULL or valid for writing a pointer. Not thread-safe (see
/// `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_nodegraph_set_input_colorspace(
    b: *mut shadewire_builder,
    ng: *const shadewire_nodegraph,
    name: *const c_char,
    colorspace: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let graph = b.graph(ng)?;
                let name = required(name, "name")?;
                let colorspace = required(colorspace, "colorspace")?;
                b.inner.set_interface_colorspace(graph, name, colorspace)?;
                Ok(())
            }),
        )
    }
}

/// Adds an output `name` of MaterialX type `type` to `ng`, reading `from` (a node of `ng` or one of its interface
/// inputs). Connections to it may be made before it is added.
///
/// # Safety
/// `b` must be NULL or a live builder; `ng` NULL or any pointer (it is looked up, not read); `name`, `type` and
/// `source_path` NULL or valid NUL-terminated strings; `from` NULL or valid (see `shadewire_source`); `error` NULL or
/// valid for writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_nodegraph_add_output(
    b: *mut shadewire_builder,
    ng: *const shadewire_nodegraph,
    name: *const c_char,
    r#type: *const c_char,
    from: *const shadewire_source,
    source_path: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let graph = b.graph(ng)?;
                let name = required(name, "name")?;
                let ty = required(r#type, "type")?;
                let from = read_source(b, from)?;
                b.inner.add_output(Some(graph), name, ty, from, opt_cstr(source_path)?)?;
                Ok(())
            }),
        )
    }
}

/// Adds a node `name` of the nodedef `nodedef` (for example "ND_image_color3") to `ng`, or to the document when `ng`
/// is NULL, and writes its handle to `out` (written only on success). Its category and output type come from the
/// nodedef; one of several outputs makes a multi-output node, read by output name. Inputs set or connected with a
/// NULL type then take theirs from the nodedef. An unknown nodedef is an error naming `source_path`.
///
/// # Safety
/// `b` must be NULL or a live builder; `ng` NULL or any pointer (it is looked up, not read); `name`, `nodedef` and
/// `source_path` NULL or valid NUL-terminated strings; `out` valid for writing a pointer; `error` NULL or valid for
/// writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_builder_add_node(
    b: *mut shadewire_builder,
    ng: *const shadewire_nodegraph,
    name: *const c_char,
    nodedef: *const c_char,
    source_path: *const c_char,
    out: *mut *mut shadewire_node,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let b = builder_mut(b)?;
                let scope = b.scope(ng)?;
                let name = required(name, "name")?;
                let nodedef = required(nodedef, "nodedef")?;
                let node = b.inner.add_node_from_nodedef(scope, nodedef, name, opt_cstr(source_path)?)?;
                *out = b.keep_node(node);
                Ok(())
            }),
        )
    }
}

/// Adds a node `name` of `category` (for example "multiply") whose output is of MaterialX type `type` to `ng`, or to
/// the document when `ng` is NULL, and writes its handle to `out` (written only on success). Its nodedef is picked
/// when the document is lowered, as for XML, so its inputs need explicit types.
///
/// # Safety
/// `b` must be NULL or a live builder; `ng` NULL or any pointer (it is looked up, not read); `category`, `type`,
/// `name` and `source_path` NULL or valid NUL-terminated strings; `out` valid for writing a pointer; `error` NULL or
/// valid for writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_builder_add_node_category(
    b: *mut shadewire_builder,
    ng: *const shadewire_nodegraph,
    category: *const c_char,
    r#type: *const c_char,
    name: *const c_char,
    source_path: *const c_char,
    out: *mut *mut shadewire_node,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let b = builder_mut(b)?;
                let scope = b.scope(ng)?;
                let category = required(category, "category")?;
                let ty = required(r#type, "type")?;
                let name = required(name, "name")?;
                let node = b.inner.add_node(scope, category, ty, name, opt_cstr(source_path)?)?;
                *out = b.keep_node(node);
                Ok(())
            }),
        )
    }
}

/// Sets `input` of `n` to `value`, as MaterialX type `type` (NULL: the type `n`'s nodedef gives it). Inputs are
/// write-once: an input already set or connected is an error.
///
/// # Safety
/// `b` must be NULL or a live builder; `n` NULL or any pointer (it is looked up, not read); `input`, `type` and
/// `source_path` NULL or valid NUL-terminated strings; `value` NULL or valid (see `shadewire_value`); `error` NULL or
/// valid for writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_node_set_value(
    b: *mut shadewire_builder,
    n: *const shadewire_node,
    input: *const c_char,
    r#type: *const c_char,
    value: *const shadewire_value,
    source_path: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let node = b.node(n)?;
                let input = required(input, "input")?;
                let ty = opt_cstr(r#type)?;
                let value = read_value(value)?;
                b.inner.set_input(node, input, ty, value, opt_cstr(source_path)?)?;
                Ok(())
            }),
        )
    }
}

/// Sets the colorspace of `input` of `n`, already set or connected, whatever its type.
///
/// # Safety
/// `b` must be NULL or a live builder; `n` NULL or any pointer (it is looked up, not read); `input` and `colorspace`
/// NULL or valid NUL-terminated strings; `error` NULL or valid for writing a pointer. Not thread-safe (see
/// `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_node_set_colorspace(
    b: *mut shadewire_builder,
    n: *const shadewire_node,
    input: *const c_char,
    colorspace: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let node = b.node(n)?;
                let input = required(input, "input")?;
                let colorspace = required(colorspace, "colorspace")?;
                b.inner.set_input_colorspace(node, input, colorspace)?;
                Ok(())
            }),
        )
    }
}

/// Connects `input` of `n`, as MaterialX type `type` (NULL: the type `n`'s nodedef gives it), to `from`: a node of
/// the same scope, an output of a nodegraph (which may be added later), or an interface input of the enclosing
/// nodegraph. Inputs are write-once: an input already set or connected is an error.
///
/// # Safety
/// `b` must be NULL or a live builder; `n` NULL or any pointer (it is looked up, not read); `input`, `type` and
/// `source_path` NULL or valid NUL-terminated strings; `from` NULL or valid (see `shadewire_source`); `error` NULL or
/// valid for writing a pointer. Not thread-safe (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_node_connect(
    b: *mut shadewire_builder,
    n: *const shadewire_node,
    input: *const c_char,
    r#type: *const c_char,
    from: *const shadewire_source,
    source_path: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let node = b.node(n)?;
                let input = required(input, "input")?;
                let ty = opt_cstr(r#type)?;
                let from = read_source(b, from)?;
                b.inner.connect(node, input, ty, from, opt_cstr(source_path)?)?;
                Ok(())
            }),
        )
    }
}

/// Adds a `surfacematerial` named `name` reading `surface`, which must be a surfaceshader node of the document itself
/// (added with a NULL nodegraph).
///
/// # Safety
/// `b` must be NULL or a live builder; `surface` NULL or any pointer (it is looked up, not read); `name` and
/// `source_path` NULL or valid NUL-terminated strings; `error` NULL or valid for writing a pointer. Not thread-safe
/// (see `shadewire_builder`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_builder_add_material(
    b: *mut shadewire_builder,
    name: *const c_char,
    surface: *const shadewire_node,
    source_path: *const c_char,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                let b = builder_mut(b)?;
                let surface = b.node(surface)?;
                let name = required(name, "name")?;
                b.inner.add_material(name, surface, opt_cstr(source_path)?)?;
                Ok(())
            }),
        )
    }
}

/// Lowers `material` (NULL for the document's only one: SHADEWIRE_ERROR_MATERIAL when it has several) to the IR, as
/// `shadewire_mtlx_to_ir` does a parsed document, and writes the handle to `out` (written only on success). `b` is
/// unchanged, so it may be edited and lowered again. `options` may be NULL for the defaults; its `resolve` is unused,
/// as a built document has no includes. The IR is not optimized.
///
/// # Safety
/// `b` must be NULL or a live builder; `material` NULL or a valid NUL-terminated string; `options` NULL or valid (see
/// `shadewire_mtlx_options`); `out` valid for writing a pointer; `error` NULL or valid for writing a pointer. Not
/// thread-safe (see `shadewire_builder`): no other call on `b` may run meanwhile.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_builder_to_ir(
    b: *const shadewire_builder,
    material: *const c_char,
    options: *const shadewire_mtlx_options,
    out: *mut *mut shadewire_ir,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let b = b.as_ref().ok_or_else(|| ArgError("builder is NULL".to_string()))?;
                let material = opt_cstr(material)?;
                let (options, _resolver) = read_mtlx(options)?;
                let ir = b.inner.to_ir(material, &options)?;
                *out = Box::into_raw(Box::new(shadewire_ir::new(ir)));
                Ok(())
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::{null, null_mut};

    fn value(kind: shadewire_value_kind) -> shadewire_value {
        shadewire_value { kind, floats: null(), count: 0, integer: 0, boolean: false, string: null() }
    }

    fn source(kind: shadewire_source_kind) -> shadewire_source {
        shadewire_source { kind, node: null(), nodegraph: null(), output: null(), interface_name: null() }
    }

    #[test]
    fn a_boolean_is_true_for_any_non_zero_byte() {
        let mut v = value(SHADEWIRE_VALUE_BOOLEAN);
        unsafe { (&raw mut v.boolean).cast::<u8>().write(2) };
        assert!(matches!(unsafe { read_value(&v) }, Ok(InputValue::Boolean(true))));
    }

    #[test]
    fn values_with_missing_pointers_or_unknown_kinds_are_refused() {
        let mut floats = value(SHADEWIRE_VALUE_FLOATS);
        floats.count = 3;
        assert!(unsafe { read_value(&floats) }.is_err());
        assert!(unsafe { read_value(&value(SHADEWIRE_VALUE_STRING)) }.is_err());
        assert!(unsafe { read_value(&value(4)) }.is_err());
        assert!(unsafe { read_value(null()) }.is_err());
        assert!(matches!(unsafe { read_value(&value(SHADEWIRE_VALUE_FLOATS)) }, Ok(InputValue::Floats(&[]))));
    }

    #[test]
    fn sources_need_their_handle_or_name() {
        let b = shadewire_builder_new();
        let builder = unsafe { &*b };
        for kind in [SHADEWIRE_SOURCE_NODE, SHADEWIRE_SOURCE_NODEGRAPH, SHADEWIRE_SOURCE_INTERFACE, 3] {
            assert!(unsafe { read_source(builder, &source(kind)) }.is_err(), "kind {kind}");
        }
        assert!(unsafe { read_source(builder, null()) }.is_err());
        unsafe { shadewire_builder_free(b) };
    }

    #[test]
    fn a_stray_pointer_is_looked_up_not_read() {
        let b = shadewire_builder_new();
        let builder = unsafe { &*b };
        let stray = 0x10 as *const shadewire_node;
        assert!(builder.node(stray).is_err());
        assert!(builder.graph(0x20 as *const shadewire_nodegraph).is_err());
        let mut out = null_mut();
        let mut error = null_mut();
        let name = c"g";
        let status = unsafe { shadewire_builder_add_nodegraph(b, name.as_ptr(), null(), null_mut(), &mut error) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
        unsafe { crate::error::shadewire_error_free(error) };
        let status = unsafe { shadewire_builder_add_nodegraph(b, name.as_ptr(), null(), &mut out, null_mut()) };
        assert_eq!(status, shadewire_status::SHADEWIRE_OK);
        assert!(unsafe { &*b }.graph(out).is_ok());
        unsafe { shadewire_builder_free(b) };
    }
}
