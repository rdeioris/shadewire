//! The `shadewire` Python module: MaterialX materials compiled to the Shadewire IR, returned as Python objects
//! (the document `shadewire ir --json` prints, see `shadewire::export`).
//!
//! Built against Python's stable ABI from 3.8 (`abi3-py38`): one build per platform imports in every CPython
//! from 3.8 on. Compiles release the GIL.

use std::path::{Path, PathBuf};

use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use shadewire::optimize::optimize;
use shadewire_mtlx::{MtlxOptions, to_ir};

create_exception!(shadewire, ShadewireError, PyException, "A MaterialX document Shadewire cannot read or compile.");

fn error(e: impl std::fmt::Display) -> PyErr {
    ShadewireError::new_err(e.to_string())
}

fn read(path: &Path) -> PyResult<String> {
    std::fs::read_to_string(path).map_err(|e| error(format!("{}: {e}", path.display())))
}

/// The IR document of `material` in `text` as JSON text.
fn ir_json(
    text: &str,
    path: Option<&Path>,
    material: Option<&str>,
    models: Option<Vec<String>>,
    constants: bool,
    optimized: bool,
) -> Result<String, String> {
    let mut options = MtlxOptions { values_as_uniforms: !constants, ..MtlxOptions::default() };
    if let Some(models) = models {
        options.models = models;
    }
    let ir = to_ir(text, path, material, &options).map_err(|e| e.to_string())?;
    let dag = if optimized { optimize(&ir.dag) } else { ir.dag };
    serde_json::to_string(&shadewire::export::export(&dag)).map_err(|e| e.to_string())
}

fn to_python(py: Python<'_>, json: &str) -> PyResult<Py<PyAny>> {
    Ok(py.import("json")?.call_method1("loads", (json,))?.unbind())
}

/// The version of the Shadewire library this module was built from.
#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The names of the materials of the MaterialX document at `path`, in document order.
#[pyfunction]
fn material_names(py: Python<'_>, path: PathBuf) -> PyResult<Vec<String>> {
    let text = read(&path)?;
    py.detach(|| shadewire_mtlx::material_names(&text, Some(&path)).map_err(|e| e.to_string())).map_err(error)
}

/// Compiles `material` (None: the document's only one) of the MaterialX document at `path` and returns its IR
/// document as dicts and lists. `models` are the surface models the caller accepts (None: Shadewire's default
/// four; include "surface" for closure trees); `constants` bakes authored values in instead of keeping them as
/// uniforms; `optimize` runs Shadewire's optimizer (folding, sharing, closure pruning).
#[pyfunction]
#[pyo3(signature = (path, material=None, models=None, constants=false, optimize=true))]
fn compile_ir(
    py: Python<'_>,
    path: PathBuf,
    material: Option<String>,
    models: Option<Vec<String>>,
    constants: bool,
    optimize: bool,
) -> PyResult<Py<PyAny>> {
    let text = read(&path)?;
    let json =
        py.detach(|| ir_json(&text, Some(&path), material.as_deref(), models, constants, optimize)).map_err(error)?;
    to_python(py, &json)
}

/// `compile_ir` for a document given as text; `path` names it (for messages and relative includes).
#[pyfunction]
#[pyo3(signature = (text, path=None, material=None, models=None, constants=false, optimize=true))]
fn compile_ir_text(
    py: Python<'_>,
    text: String,
    path: Option<PathBuf>,
    material: Option<String>,
    models: Option<Vec<String>>,
    constants: bool,
    optimize: bool,
) -> PyResult<Py<PyAny>> {
    let json = py
        .detach(|| ir_json(&text, path.as_deref(), material.as_deref(), models, constants, optimize))
        .map_err(error)?;
    to_python(py, &json)
}

#[pymodule]
#[pyo3(name = "shadewire")]
fn shadewire_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ShadewireError", m.py().get_type::<ShadewireError>())?;
    m.add("IR_FORMAT_VERSION", shadewire::export::FORMAT_VERSION)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(material_names, m)?)?;
    m.add_function(wrap_pyfunction!(compile_ir, m)?)?;
    m.add_function(wrap_pyfunction!(compile_ir_text, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ir_json;

    #[test]
    fn a_surface_document_exports_its_closure_tree() {
        let text = r#"<materialx version="1.39">
          <oren_nayar_diffuse_bsdf name="d" type="BSDF" />
          <surface name="s" type="surfaceshader"><input name="bsdf" type="BSDF" nodename="d" /></surface>
          <surfacematerial name="M" type="material"><input name="surfaceshader" type="surfaceshader" nodename="s" /></surfacematerial>
        </materialx>"#;
        let json = ir_json(text, None, None, Some(vec!["surface".to_string()]), true, true).unwrap();
        assert!(json.starts_with(r#"{"format":"shadewire-ir","version":1,"model":"surface""#), "{json}");
        assert!(json.contains(r#""kind":"oren_nayar_diffuse_bsdf""#), "{json}");
    }

    #[test]
    fn errors_are_messages() {
        let err = ir_json("<materialx version=\"1.39\"/>", None, None, None, false, true).unwrap_err();
        assert!(err.contains("no material"), "{err}");
    }
}
