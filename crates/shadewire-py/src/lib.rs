//! The `shadewire` Python module, laid out like the Rust crates:
//!
//! - `shadewire.mtlx` (`shadewire_mtlx`): the MaterialX front end, `to_ir`, `material_names` and
//!   `list_dependencies`, with includes read by a Python `resolver`;
//! - `shadewire.Ir` (`shadewire::Dag` and its `SourceInfo`): the IR, with `optimize`, `dump` and `to_dict`
//!   (the document `shadewire::export` writes);
//! - `shadewire.wgsl`, `shadewire.glsl`, `shadewire.hlsl`, `shadewire.msl` (`shadewire_wgsl` ...): the back ends,
//!   each a `compile` to a `shadewire.Compiled` (the code and its reflection).
//!
//! Built against Python's stable ABI from 3.8 (`abi3-py38`): one build per platform imports in every CPython
//! from 3.8 on. Front-end and back-end calls release the GIL.

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use shadewire::shader::CompileOptions;
use shadewire::{Dag, SourceInfo, model_struct_name};
use shadewire_mtlx::{MtlxOptions, Resolved, Resolver};

create_exception!(shadewire, ShadewireError, PyException, "A document Shadewire cannot read or compile.");

fn error(e: impl std::fmt::Display) -> PyErr {
    ShadewireError::new_err(e.to_string())
}

/// JSON text as Python objects.
fn to_python(py: Python<'_>, json: &str) -> PyResult<Py<PyAny>> {
    Ok(py.import("json")?.call_method1("loads", (json,))?.unbind())
}

fn json<T: serde::Serialize>(value: &T) -> PyResult<String> {
    serde_json::to_string(value).map_err(error)
}

// ---------------------------------------------------------------------------------------------------- the IR

/// A material in Shadewire's IR: a typed DAG computing a shading model's fields (and, for the `surface` model,
/// the closure tree over them). Front ends make it; back ends compile it.
#[pyclass(name = "Ir", module = "shadewire", frozen)]
struct PyIr {
    dag: Dag,
    source: SourceInfo,
}

#[pymethods]
impl PyIr {
    /// The IR with Shadewire's optimizer run over it: constants folded, identities removed, equal nodes
    /// shared, closures that constant weights turn off pruned, unused nodes dropped.
    fn optimize(&self, py: Python<'_>) -> PyIr {
        let dag = py.detach(|| shadewire::optimize::optimize(&self.dag));
        PyIr { dag, source: self.source.clone() }
    }

    /// One line per node, field, closure node and approximation: the text `shadewire ir` prints.
    fn dump(&self) -> String {
        self.dag.dump()
    }

    /// The IR as dicts and lists: nodes, textures, fields, the closure tree (`shadewire ir --json`).
    fn to_dict(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &json(&shadewire::export::export(&self.dag))?)
    }

    /// The shading model: `surface` (a closure tree), `standard_surface`, `open_pbr_surface`, `gltf_pbr`, ...
    #[getter]
    fn model(&self) -> &str {
        &self.dag.surface.model
    }

    /// The front end that made it (`mtlx`) and the document's version.
    #[getter]
    fn source(&self) -> (String, String) {
        (self.source.source.clone(), self.source.mtlx_version.clone())
    }

    /// The structure hash: equal for materials whose generated code is equal (uniform values left out).
    #[getter]
    fn structure_hash(&self) -> String {
        shadewire::structure_hash(&self.dag)
    }

    fn __repr__(&self) -> String {
        format!(
            "<shadewire.Ir model={} nodes={} fields={}>",
            self.dag.surface.model,
            self.dag.nodes.len(),
            self.dag.surface.fields.len()
        )
    }
}

// ---------------------------------------------------------------------------------------------------- mtlx

/// A Python `resolver(base, href) -> (location, text)` as the front end's `Resolver` (None: the file system). The
/// front end runs with the GIL released, so each call takes it back; an exception's message is the error.
fn resolver(callable: Option<Py<PyAny>>) -> Resolver {
    let Some(callable) = callable else {
        return Resolver::default();
    };
    Resolver::new(move |base, href| {
        Python::attach(|py| {
            let answer = callable.bind(py).call1((base, href)).and_then(|r| r.extract::<(String, String)>());
            answer.map(|(location, text)| Resolved { location, text }).map_err(|e| e.value(py).to_string())
        })
    })
}

/// MaterialX text to the IR (not optimized). `location` names the document, for messages and as the base its
/// includes resolve against: an OS path for the default resolver, any string `resolver` understands otherwise.
/// `resolver(base, href)` returns an include's `(location, text)` and raises to fail (None reads the file system
/// relative to `base`). `material` picks one (None: the document's only one); `models` are the shading models the
/// caller accepts (None: `open_pbr_surface`, `standard_surface`, `gltf_pbr`, `surface_unlit`; add `surface` for
/// closure trees, which also opens the named models not listed); `values_as_uniforms` keeps authored values as
/// uniforms (False bakes them in, so the optimizer can prune by them).
#[pyfunction]
#[pyo3(signature = (text, location=None, material=None, models=None, working_colorspace=None, values_as_uniforms=true, resolver=None))]
#[allow(clippy::too_many_arguments)]
fn to_ir(
    py: Python<'_>,
    text: String,
    location: Option<String>,
    material: Option<String>,
    models: Option<Vec<String>>,
    working_colorspace: Option<String>,
    values_as_uniforms: bool,
    resolver: Option<Py<PyAny>>,
) -> PyResult<PyIr> {
    let mut options = MtlxOptions { values_as_uniforms, resolver: self::resolver(resolver), ..MtlxOptions::default() };
    if let Some(models) = models {
        options.models = models;
    }
    if let Some(colorspace) = working_colorspace {
        options.working_colorspace = colorspace;
    }
    let ir = py
        .detach(|| shadewire_mtlx::to_ir(&text, location.as_deref(), material.as_deref(), &options))
        .map_err(error)?;
    Ok(PyIr { dag: ir.dag, source: ir.source })
}

/// The names of a MaterialX document's materials, in document order. `location` and `resolver` as `to_ir`'s.
#[pyfunction]
#[pyo3(signature = (text, location=None, resolver=None))]
fn material_names(
    py: Python<'_>,
    text: String,
    location: Option<String>,
    resolver: Option<Py<PyAny>>,
) -> PyResult<Vec<String>> {
    let resolver = self::resolver(resolver);
    py.detach(|| shadewire_mtlx::material_names(&text, location.as_deref(), &resolver)).map_err(error)
}

/// What a document reads besides itself, unresolved and in document order: `{"includes": [...], "files": [...]}`,
/// the `xi:include` hrefs (not followed: list each included document again once fetched) and the `filename` input
/// values with `fileprefix` applied. A host with its own I/O fetches them all, then compiles with a `resolver`
/// serving them from memory. `location` only names the document in errors.
#[pyfunction]
#[pyo3(signature = (text, location=None))]
fn list_dependencies(py: Python<'_>, text: String, location: Option<String>) -> PyResult<Py<PyAny>> {
    let deps = py.detach(|| shadewire_mtlx::list_dependencies(&text, location.as_deref())).map_err(error)?;
    let dict = pyo3::types::PyDict::new(py);
    dict.set_item("includes", deps.includes)?;
    dict.set_item("files", deps.files)?;
    Ok(dict.into_any().unbind())
}

// ---------------------------------------------------------------------------------------------------- back ends

/// A compiled material: the generated code in its language (the model's struct, `shadewire_eval_surface()`, the
/// closure functions, declarations of the host functions it calls), what a host must bind and provide (the
/// reflection), and the IR the code was printed from (optimized when the back end was asked to).
#[pyclass(name = "Compiled", module = "shadewire", frozen)]
struct PyCompiled {
    language: &'static str,
    code: String,
    reflection: shadewire::Reflection,
    dag: Dag,
    source: SourceInfo,
}

#[pymethods]
impl PyCompiled {
    /// `wgsl`, `glsl`, `hlsl` or `msl`.
    #[getter]
    fn language(&self) -> &str {
        self.language
    }

    #[getter]
    fn code(&self) -> &str {
        &self.code
    }

    /// Bindings, uniforms (offsets, defaults), textures, the host functions the code calls (`context`), each
    /// field's kind, and the closure tree of a `surface` material.
    #[getter]
    fn reflection(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &json(&self.reflection)?)
    }

    /// The name of the struct `shadewire_eval_surface` returns (`Surface`, `StandardSurface`, ...).
    #[getter]
    fn model_struct(&self) -> String {
        model_struct_name(&self.reflection.model)
    }

    /// The IR the code was printed from.
    #[getter]
    fn ir(&self) -> PyIr {
        PyIr { dag: self.dag.clone(), source: self.source.clone() }
    }

    fn __repr__(&self) -> String {
        format!("<shadewire.Compiled {} model={} {} bytes>", self.language, self.reflection.model, self.code.len())
    }
}

/// The options every back end takes.
fn options(
    group: u32,
    first_binding: u32,
    reserve_uniform_binding: bool,
    flip_v: bool,
    optimize: bool,
    implicit_derivatives: bool,
) -> CompileOptions {
    CompileOptions { group, first_binding, reserve_uniform_binding, flip_v, optimize, implicit_derivatives }
}

/// Runs a back end with the GIL released.
fn run(
    py: Python<'_>,
    language: &'static str,
    ir: &PyIr,
    options: CompileOptions,
    backend: impl FnOnce(&Dag, &SourceInfo, &CompileOptions) -> Result<shadewire::shader::Compiled, shadewire::Error> + Send,
) -> PyResult<PyCompiled> {
    let compiled = py.detach(|| backend(&ir.dag, &ir.source, &options)).map_err(error)?;
    Ok(PyCompiled {
        language,
        code: compiled.code,
        reflection: compiled.reflection,
        dag: compiled.dag,
        source: ir.source.clone(),
    })
}

/// The IR to WGSL. `group` and `first_binding` place the uniform block, textures and samplers;
/// `reserve_uniform_binding` keeps `first_binding` for the uniform block even without uniforms; `flip_v`
/// flips V when sampling (images uploaded top row first); `optimize` runs the optimizer first;
/// `implicit_derivatives` False makes the code run in a compute stage (level-0 samples, zero derivatives).
#[pyfunction]
#[pyo3(name = "compile", signature = (ir, group=2, first_binding=0, reserve_uniform_binding=false, flip_v=true, optimize=true, implicit_derivatives=true))]
#[allow(clippy::too_many_arguments)]
fn compile_wgsl(
    py: Python<'_>,
    ir: &PyIr,
    group: u32,
    first_binding: u32,
    reserve_uniform_binding: bool,
    flip_v: bool,
    optimize: bool,
    implicit_derivatives: bool,
) -> PyResult<PyCompiled> {
    let options = options(group, first_binding, reserve_uniform_binding, flip_v, optimize, implicit_derivatives);
    run(py, "wgsl", ir, options, shadewire_wgsl::compile)
}

/// The IR to a GLSL library: `version` is the `#version` (desktop 330 and 400 to 460, default 420; with `es`, 300
/// to 320, default 300; any other is a `ShadewireError`). From GLSL 420 and ES 310 the uniform block sits at binding point `uniform_binding` and each
/// texture (a `sampler2D`) at unit `binding`, as reflected; before, bind them by name. The other options as
/// `shadewire.wgsl.compile`.
#[pyfunction]
#[pyo3(name = "compile", signature = (ir, version=None, es=false, group=2, first_binding=0, reserve_uniform_binding=false, flip_v=true, optimize=true, implicit_derivatives=true))]
#[allow(clippy::too_many_arguments)]
fn compile_glsl(
    py: Python<'_>,
    ir: &PyIr,
    version: Option<u16>,
    es: bool,
    group: u32,
    first_binding: u32,
    reserve_uniform_binding: bool,
    flip_v: bool,
    optimize: bool,
    implicit_derivatives: bool,
) -> PyResult<PyCompiled> {
    let version = shadewire_glsl::Version::new(es, version);
    let options = options(group, first_binding, reserve_uniform_binding, flip_v, optimize, implicit_derivatives);
    run(py, "glsl", ir, options, |dag, source, options| shadewire_glsl::compile(dag, source, options, version))
}

/// The IR to an HLSL library (shader model 5.1 and up): resources at `register(b|t|s{binding}, space{group})`, as
/// reflected. The options as `shadewire.wgsl.compile`.
#[pyfunction]
#[pyo3(name = "compile", signature = (ir, group=2, first_binding=0, reserve_uniform_binding=false, flip_v=true, optimize=true, implicit_derivatives=true))]
#[allow(clippy::too_many_arguments)]
fn compile_hlsl(
    py: Python<'_>,
    ir: &PyIr,
    group: u32,
    first_binding: u32,
    reserve_uniform_binding: bool,
    flip_v: bool,
    optimize: bool,
    implicit_derivatives: bool,
) -> PyResult<PyCompiled> {
    let options = options(group, first_binding, reserve_uniform_binding, flip_v, optimize, implicit_derivatives);
    run(py, "hlsl", ir, options, shadewire_hlsl::compile)
}

/// The IR to a Metal Shading Language library: the functions that read the material's resources take them as
/// trailing parameters (the uniform block, then each texture and sampler). The options as
/// `shadewire.wgsl.compile`.
#[pyfunction]
#[pyo3(name = "compile", signature = (ir, group=2, first_binding=0, reserve_uniform_binding=false, flip_v=true, optimize=true, implicit_derivatives=true))]
#[allow(clippy::too_many_arguments)]
fn compile_msl(
    py: Python<'_>,
    ir: &PyIr,
    group: u32,
    first_binding: u32,
    reserve_uniform_binding: bool,
    flip_v: bool,
    optimize: bool,
    implicit_derivatives: bool,
) -> PyResult<PyCompiled> {
    let options = options(group, first_binding, reserve_uniform_binding, flip_v, optimize, implicit_derivatives);
    run(py, "msl", ir, options, shadewire_msl::compile)
}

// ---------------------------------------------------------------------------------------------------- the module

/// The library version.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `name` as a submodule of `parent`, importable as `shadewire.<name>`.
fn submodule<'py>(parent: &Bound<'py, PyModule>, name: &str) -> PyResult<Bound<'py, PyModule>> {
    let py = parent.py();
    let module = PyModule::new(py, name)?;
    parent.add_submodule(&module)?;
    py.import("sys")?.getattr("modules")?.set_item(format!("shadewire.{name}"), &module)?;
    Ok(module)
}

#[pymodule]
#[pyo3(name = "_native")]
fn shadewire_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ShadewireError", m.py().get_type::<ShadewireError>())?;
    m.add("IR_FORMAT_VERSION", shadewire::export::FORMAT_VERSION)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_class::<PyIr>()?;

    let mtlx = submodule(m, "mtlx")?;
    mtlx.add_function(wrap_pyfunction!(to_ir, &mtlx)?)?;
    mtlx.add_function(wrap_pyfunction!(material_names, &mtlx)?)?;
    mtlx.add_function(wrap_pyfunction!(list_dependencies, &mtlx)?)?;

    m.add_class::<PyCompiled>()?;
    let wgsl = submodule(m, "wgsl")?;
    wgsl.add_function(wrap_pyfunction!(compile_wgsl, &wgsl)?)?;
    let glsl = submodule(m, "glsl")?;
    glsl.add_function(wrap_pyfunction!(compile_glsl, &glsl)?)?;
    let hlsl = submodule(m, "hlsl")?;
    hlsl.add_function(wrap_pyfunction!(compile_hlsl, &hlsl)?)?;
    let msl = submodule(m, "msl")?;
    msl.add_function(wrap_pyfunction!(compile_msl, &msl)?)?;
    Ok(())
}
