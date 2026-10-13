//! The `shadewire` Python module, laid out like the Rust crates:
//!
//! - `shadewire.mtlx` (`shadewire_mtlx`): the MaterialX front end, `to_ir` and `material_names`;
//! - `shadewire.Ir` (`shadewire::Dag` and its `SourceInfo`): the IR, with `optimize`, `dump` and `to_dict`
//!   (the document `shadewire::export` writes);
//! - `shadewire.wgsl` (`shadewire_wgsl`): the WGSL back end, `compile` (to a `Compiled` with the WGSL and its
//!   reflection), `link`, `translate` and `reference_prelude`.
//!
//! Built against Python's stable ABI from 3.8 (`abi3-py38`): one build per platform imports in every CPython
//! from 3.8 on. Front-end and back-end calls release the GIL.

use pyo3::create_exception;
use pyo3::exceptions::{PyException, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use shadewire::{Dag, SourceInfo, model_struct_name};
use shadewire_mtlx::MtlxOptions;
use shadewire_wgsl::{CompileOptions, GlslVersion, HlslShaderModel, MslVersion, Output, Target};

create_exception!(shadewire, ShadewireError, PyException, "A document Shadewire cannot read, compile or link.");

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

/// MaterialX text to the IR (not optimized). `path` names the document, for messages and relative includes;
/// `material` picks one (None: the document's only one); `models` are the shading models the caller accepts
/// (None: `open_pbr_surface`, `standard_surface`, `gltf_pbr`, `surface_unlit`; add `surface` for closure
/// trees, which also opens the named models not listed); `values_as_uniforms` keeps authored values as
/// uniforms (False bakes them in, so the optimizer can prune by them).
#[pyfunction]
#[pyo3(signature = (text, path=None, material=None, models=None, working_colorspace=None, values_as_uniforms=true))]
fn to_ir(
    py: Python<'_>,
    text: String,
    path: Option<std::path::PathBuf>,
    material: Option<String>,
    models: Option<Vec<String>>,
    working_colorspace: Option<String>,
    values_as_uniforms: bool,
) -> PyResult<PyIr> {
    let mut options = MtlxOptions { values_as_uniforms, ..MtlxOptions::default() };
    if let Some(models) = models {
        options.models = models;
    }
    if let Some(colorspace) = working_colorspace {
        options.working_colorspace = colorspace;
    }
    let ir =
        py.detach(|| shadewire_mtlx::to_ir(&text, path.as_deref(), material.as_deref(), &options)).map_err(error)?;
    Ok(PyIr { dag: ir.dag, source: ir.source })
}

/// The names of a MaterialX document's materials, in document order.
#[pyfunction]
#[pyo3(signature = (text, path=None))]
fn material_names(py: Python<'_>, text: String, path: Option<std::path::PathBuf>) -> PyResult<Vec<String>> {
    py.detach(|| shadewire_mtlx::material_names(&text, path.as_deref())).map_err(error)
}

// ---------------------------------------------------------------------------------------------------- wgsl

/// A compiled material: the generated WGSL (`fn shadewire_eval_surface() -> <model_struct>`) and what a host
/// must bind and provide (the reflection).
#[pyclass(name = "Compiled", module = "shadewire.wgsl", frozen)]
struct PyCompiled {
    wgsl: String,
    reflection: shadewire::Reflection,
}

#[pymethods]
impl PyCompiled {
    #[getter]
    fn wgsl(&self) -> &str {
        &self.wgsl
    }

    /// Bindings, uniforms (offsets, defaults), textures, the host functions the code calls (`context`), each
    /// field's kind, and the closure tree of a `surface` material.
    #[getter]
    fn reflection(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        to_python(py, &json(&self.reflection)?)
    }

    /// The WGSL name of the struct `shadewire_eval_surface` returns (`Surface`, `StandardSurface`, ...).
    #[getter]
    fn model_struct(&self) -> String {
        model_struct_name(&self.reflection.model)
    }

    fn __repr__(&self) -> String {
        format!("<shadewire.wgsl.Compiled model={} {} bytes of WGSL>", self.reflection.model, self.wgsl.len())
    }
}

/// The IR to WGSL. `group` and `first_binding` place the uniform block, textures and samplers;
/// `reserve_uniform_binding` keeps `first_binding` for the uniform block even without uniforms; `flip_v`
/// flips V when sampling (images uploaded top row first); `optimize` runs the optimizer first;
/// `implicit_derivatives` False makes the code run in a compute stage (level-0 samples, zero derivatives).
#[pyfunction]
#[pyo3(signature = (ir, group=2, first_binding=0, reserve_uniform_binding=false, flip_v=true, optimize=true, implicit_derivatives=true))]
#[allow(clippy::too_many_arguments)]
fn compile(
    py: Python<'_>,
    ir: &PyIr,
    group: u32,
    first_binding: u32,
    reserve_uniform_binding: bool,
    flip_v: bool,
    optimize: bool,
    implicit_derivatives: bool,
) -> PyResult<PyCompiled> {
    let options =
        CompileOptions { group, first_binding, reserve_uniform_binding, flip_v, optimize, implicit_derivatives };
    let compiled = py.detach(|| shadewire_wgsl::compile(&ir.dag, &ir.source, &options)).map_err(error)?;
    Ok(PyCompiled { wgsl: compiled.wgsl, reflection: compiled.reflection })
}

/// `host_prelude` and `generated` joined and validated with naga; the joined module, or a ShadewireError with
/// naga's message (line and caret) when it does not validate.
#[pyfunction]
fn link(py: Python<'_>, generated: String, host_prelude: String) -> PyResult<String> {
    py.detach(|| shadewire_wgsl::link(&generated, &host_prelude)).map_err(error)
}

/// A complete, simple host for a compiled material: every host function returns a private variable (zero unless
/// set), transforms are identities, and a fragment entry point returns the first color field. A template for
/// writing your own; it knows nothing of closures.
#[pyfunction]
fn reference_prelude(compiled: &PyCompiled) -> String {
    shadewire_wgsl::reference_host::prelude(&compiled.reflection, &model_struct_name(&compiled.reflection.model))
}

/// The compiled material as a GLSL library, for hosts written in GLSL: the surface struct, the uniform block and
/// samplers (at binding points `32 * group + binding`), `shadewire_eval_surface()`, and a prototype for each host
/// function it calls, under the reflection's names. Append the host's GLSL (the host functions, lighting, `main`)
/// and compile the whole as a fragment shader. `version`: GLSL 420..460 (GLSL ES 310 or 320 with `es`).
#[pyfunction]
#[pyo3(signature = (compiled, version=420, es=false))]
fn glsl_library(py: Python<'_>, compiled: &PyCompiled, version: u16, es: bool) -> PyResult<String> {
    let version = if es { GlslVersion::Es(version) } else { GlslVersion::Desktop(version) };
    py.detach(|| shadewire_wgsl::glsl_library(&compiled.wgsl, &compiled.reflection, version)).map_err(error)
}

fn target(name: &str, version: Option<u32>) -> PyResult<Target> {
    let hlsl = |v: u32| match v {
        50 => Some(HlslShaderModel::Sm5_0),
        51 => Some(HlslShaderModel::Sm5_1),
        60 => Some(HlslShaderModel::Sm6_0),
        61 => Some(HlslShaderModel::Sm6_1),
        62 => Some(HlslShaderModel::Sm6_2),
        63 => Some(HlslShaderModel::Sm6_3),
        64 => Some(HlslShaderModel::Sm6_4),
        65 => Some(HlslShaderModel::Sm6_5),
        66 => Some(HlslShaderModel::Sm6_6),
        67 => Some(HlslShaderModel::Sm6_7),
        68 => Some(HlslShaderModel::Sm6_8),
        69 => Some(HlslShaderModel::Sm6_9),
        _ => None,
    };
    let bad = || PyValueError::new_err(format!("unsupported version {version:?} for {name}"));
    Ok(match name {
        "wgsl" => Target::Wgsl,
        "hlsl" => Target::Hlsl(hlsl(version.unwrap_or(60)).ok_or_else(bad)?),
        "glsl" => Target::Glsl(GlslVersion::Desktop(version.unwrap_or(450) as u16)),
        "glsl-es" => Target::Glsl(GlslVersion::Es(version.unwrap_or(300) as u16)),
        "webgl2" => Target::Glsl(GlslVersion::WebGl2),
        "msl" => {
            let v = version.unwrap_or(21);
            Target::Msl(MslVersion { major: (v / 10) as u8, minor: (v % 10) as u8 })
        }
        "spirv" => Target::SpirV,
        other => {
            return Err(PyValueError::new_err(format!(
                "unknown target {other}: wgsl, hlsl, glsl, glsl-es, webgl2, msl or spirv"
            )));
        }
    })
}

/// A linked module (with an entry point, see `link`) in another language: `hlsl` (version 50..69, default 60),
/// `glsl` (330..450, default 450), `glsl-es` (300 or 310), `webgl2`, `msl` (major * 10 + minor, default 21),
/// `spirv` (bytes) or `wgsl` (naga's rewrite).
///
/// `entry_point` picks the entry point to write (GLSL, HLSL and SPIR-V write one; None: the first fragment
/// entry point). GLSL follows OpenGL's conventions: clip-space depth from -1 to 1, and `@group(g)
/// @binding(b)` at binding point `32 * g + b` (GLSL 4.20 / ES 3.10; a texture and its sampler combine into one
/// `sampler2D` at the texture's point).
#[pyfunction]
#[pyo3(signature = (linked, target_name, version=None, entry_point=None))]
fn translate(
    py: Python<'_>,
    linked: String,
    target_name: &str,
    version: Option<u32>,
    entry_point: Option<String>,
) -> PyResult<Py<PyAny>> {
    let target = target(target_name, version)?;
    match py.detach(|| shadewire_wgsl::translate(&linked, target, entry_point.as_deref())).map_err(error)? {
        Output::Text(text) => Ok(text.into_pyobject(py)?.into_any().unbind()),
        Output::Words(words) => {
            let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
            Ok(PyBytes::new(py, &bytes).into_any().unbind())
        }
    }
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

    let wgsl = submodule(m, "wgsl")?;
    wgsl.add_class::<PyCompiled>()?;
    wgsl.add_function(wrap_pyfunction!(compile, &wgsl)?)?;
    wgsl.add_function(wrap_pyfunction!(link, &wgsl)?)?;
    wgsl.add_function(wrap_pyfunction!(reference_prelude, &wgsl)?)?;
    wgsl.add_function(wrap_pyfunction!(translate, &wgsl)?)?;
    wgsl.add_function(wrap_pyfunction!(glsl_library, &wgsl)?)?;
    Ok(())
}
