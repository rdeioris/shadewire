//! The IR, as the opaque `shadewire_ir` handle (Python's `shadewire.Ir`), and the MaterialX front end that makes it
//! (`shadewire.mtlx.to_ir`).

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;

use shadewire_mtlx::MaterialIr;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::names::shadewire_strings;
use crate::options::{read_mtlx, shadewire_mtlx_options};
use crate::status::shadewire_status;
use crate::strings::{ArgError, c_string, cached_out, null_str, opt_cstr, text_arg};

/// The version of the document `shadewire_ir_json` writes (its `version` field).
pub const SHADEWIRE_IR_FORMAT_VERSION: u32 = 1;

const _: () = assert!(SHADEWIRE_IR_FORMAT_VERSION == shadewire::export::FORMAT_VERSION, "update the format version");

/// A material in Shadewire's IR: a typed DAG computing a shading model's fields (and, for the `surface` model, the
/// closure tree over them). Made by `shadewire_mtlx_to_ir` or `shadewire_blender_to_ir`, compiled by
/// `shadewire_compile`. Free it with `shadewire_ir_free`.
pub struct shadewire_ir {
    pub(crate) ir: MaterialIr,
    model: CString,
    source: CString,
    source_version: CString,
    dump: OnceLock<CString>,
    json: OnceLock<CString>,
    structure_hash: OnceLock<CString>,
}

impl shadewire_ir {
    pub(crate) fn new(ir: MaterialIr) -> shadewire_ir {
        shadewire_ir {
            model: c_string(ir.dag.surface.model.clone()),
            source: c_string(ir.source.source.clone()),
            source_version: c_string(ir.source.mtlx_version.clone()),
            ir,
            dump: OnceLock::new(),
            json: OnceLock::new(),
            structure_hash: OnceLock::new(),
        }
    }
}

/// Borrows the IR behind `ptr`.
///
/// # Safety
/// `ptr` must be NULL or a live handle that outlives `'a`.
pub(crate) unsafe fn borrow<'a>(ptr: *const shadewire_ir) -> Result<&'a MaterialIr, ArgError> {
    match unsafe { ptr.as_ref() } {
        Some(handle) => Ok(&handle.ir),
        None => Err(ArgError("ir is NULL".to_string())),
    }
}

/// Lowers MaterialX `text` (`len` bytes of UTF-8, not NUL-terminated; NULL is SHADEWIRE_ERROR_INVALID_ARGUMENT) to the
/// IR and writes the handle to `out`.
///
/// `location` (NUL-terminated, may be NULL) names the document in messages and is the base its includes resolve
/// against: a path for the default resolver, any string the host's `resolve` understands otherwise. `material`
/// selects a material by name (NULL for the document's only one: SHADEWIRE_ERROR_MATERIAL when it has several);
/// `options` may be NULL for the defaults. The IR is not optimized (`shadewire_ir_optimize`; `shadewire_compile`
/// optimizes by default). `*out` is written only on success.
///
/// # Safety
/// `text` must hold `len` readable bytes (NULL is reported, not dereferenced); `location` and `material` must be NULL
/// or valid NUL-terminated strings; `options` must be NULL or valid (see `shadewire_mtlx_options`); `out` must be valid
/// for writing a pointer and `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mtlx_to_ir(
    text: *const c_char,
    len: usize,
    location: *const c_char,
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
                let text = text_arg(text, len)?;
                let location = opt_cstr(location)?;
                let material = opt_cstr(material)?;
                let (options, resolver) = read_mtlx(options)?;
                let ir = shadewire_mtlx::to_ir(text, location, material, &options, &resolver)?;
                *out = Box::into_raw(Box::new(shadewire_ir::new(ir)));
                Ok(())
            }),
        )
    }
}

/// The IR with Shadewire's optimizer run over it (constants folded, identities removed, equal nodes shared, closures
/// that constant weights turn off pruned, unused nodes dropped), as a new handle written to `out`; `ir` is unchanged.
///
/// # Safety
/// `ir` must be a live handle (NULL is an error); `out` valid for writing a pointer; `error` NULL or valid for writing
/// a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_optimize(
    ir: *const shadewire_ir,
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
                let ir = borrow(ir)?;
                let dag = shadewire::optimize::optimize(&ir.dag);
                *out = Box::into_raw(Box::new(shadewire_ir::new(MaterialIr {
                    dag,
                    source: ir.source.clone(),
                    warnings: ir.warnings.clone(),
                })));
                Ok(())
            }),
        )
    }
}

/// The shading model name (for example "standard_surface"), valid until `ir` is freed; NULL when `ir` is NULL.
///
/// # Safety
/// `ir` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_model(ir: *const shadewire_ir) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => handle.model.as_ptr(),
        None => std::ptr::null(),
    }
}

/// The front end that made the IR (`mtlx`, `blender`), valid until `ir` is freed; NULL when `ir` is NULL.
///
/// # Safety
/// `ir` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_source(ir: *const shadewire_ir) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => handle.source.as_ptr(),
        None => std::ptr::null(),
    }
}

/// The version of the document the IR was made from (for example "1.39"; for a Blender graph, the Blender that wrote
/// it), valid until `ir` is freed; NULL when `ir` is NULL.
///
/// # Safety
/// `ir` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_source_version(ir: *const shadewire_ir) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => handle.source_version.as_ptr(),
        None => std::ptr::null(),
    }
}

/// The structure hash: equal for materials whose generated code is equal (uniform values left out), so a host can
/// share one shader between them. Built on first call and valid until `ir` is freed; NULL when `ir` is NULL.
///
/// # Safety
/// `ir` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_structure_hash(ir: *const shadewire_ir) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => unsafe {
            cached_out(&handle.structure_hash, || shadewire::structure_hash(&handle.ir.dag), std::ptr::null_mut())
        },
        None => std::ptr::null(),
    }
}

/// One line per node, field, closure node and approximation (the text `shadewire ir` prints), built on first call and
/// valid until `ir` is freed; writes its byte length to `len` when non-null. NULL when `ir` is NULL or building the
/// dump failed (len is then 0).
///
/// # Safety
/// `ir` must be NULL or a live handle; `len` NULL or valid for writing a `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_dump(ir: *const shadewire_ir, len: *mut usize) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => unsafe { cached_out(&handle.dump, || handle.ir.dag.dump(), len) },
        None => unsafe { null_str(len) },
    }
}

/// The IR as a JSON document (nodes, textures, fields, the closure tree; format SHADEWIRE_IR_FORMAT_VERSION): what
/// `shadewire ir --json` prints and Python's `Ir.to_dict` returns. A `sample` node whose lookup takes explicit
/// gradients carries `grad`, the indices of its `[dx, dy]` nodes; without it the GPU derives them from `uv`.
/// Built on first call and valid until `ir` is freed; see `shadewire_ir_dump` for `len` and NULL.
///
/// # Safety
/// As `shadewire_ir_dump`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_json(ir: *const shadewire_ir, len: *mut usize) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => {
            let build =
                || serde_json::to_string(&shadewire::export::export(&handle.ir.dag)).expect("the IR serializes");
            unsafe { cached_out(&handle.json, build, len) }
        }
        None => unsafe { null_str(len) },
    }
}

/// What the front end dropped or approximated, one line each, as a `shadewire_strings` written to `out` (empty for a
/// MaterialX document today). `*out` is written only on success.
///
/// # Safety
/// `ir` must be a live handle (NULL is an error); `out` valid for writing a pointer; `error` NULL or valid for writing
/// a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_warnings(
    ir: *const shadewire_ir,
    out: *mut *mut shadewire_strings,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let ir = borrow(ir)?;
                *out = Box::into_raw(Box::new(shadewire_strings::new(ir.warnings.clone())));
                Ok(())
            }),
        )
    }
}

/// Frees `ir`; NULL is allowed.
///
/// # Safety
/// `ir` must be NULL or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_free(ir: *mut shadewire_ir) {
    if !ir.is_null() {
        drop(unsafe { Box::from_raw(ir) });
    }
}
