//! A written MaterialX document (Python's `shadewire.mtlx.from_ir`), as the opaque `shadewire_document` handle.

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;

use shadewire_mtlx::WriteOptions;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::{borrow, shadewire_ir};
use crate::status::shadewire_status;
use crate::strings::{c_string, null_str, opt_cstr, out_str};

/// A MaterialX document written from an IR: its text and the warnings the writer raised. Free it with
/// `shadewire_document_free`.
pub struct shadewire_document {
    text: CString,
    warnings: Vec<CString>,
}

/// Writes `ir` as a MaterialX document and writes the handle to `out`; `ir` is unchanged.
///
/// `inline_nodes` writes library nodes as their expansions instead of by name. `name` (NUL-terminated, may be NULL
/// for "material") names the `surfacematerial`; the nodegraph is `NG_` and that name. The shadewire definitions the
/// document needs are embedded in it. `*out` is written only on success.
///
/// # Safety
/// `ir` must be a live handle (NULL is an error); `name` must be NULL or a valid NUL-terminated string; `out` must be
/// valid for writing a pointer and `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_to_mtlx(
    ir: *const shadewire_ir,
    inline_nodes: bool,
    name: *const c_char,
    out: *mut *mut shadewire_document,
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
                let name = opt_cstr(name)?.unwrap_or("material");
                let options = WriteOptions { inline: inline_nodes, name: name.to_string(), ..WriteOptions::default() };
                let written = shadewire_mtlx::from_ir(&ir.dag, &options)?;
                let warnings = written.warnings.into_iter().map(c_string).collect();
                *out = Box::into_raw(Box::new(shadewire_document { text: c_string(written.text), warnings }));
                Ok(())
            }),
        )
    }
}

/// The document's text, valid until `doc` is freed, its byte length written to `len` when non-NULL; NULL (and 0) when
/// `doc` is NULL.
///
/// # Safety
/// `doc` must be NULL or a live handle; `len` NULL or valid for writing a `usize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_document_text(doc: *const shadewire_document, len: *mut usize) -> *const c_char {
    match unsafe { doc.as_ref() } {
        Some(doc) => unsafe { out_str(&doc.text, len) },
        None => unsafe { null_str(len) },
    }
}

/// The number of warnings; 0 when `doc` is NULL.
///
/// # Safety
/// `doc` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_document_warning_count(doc: *const shadewire_document) -> usize {
    unsafe { doc.as_ref() }.map_or(0, |doc| doc.warnings.len())
}

/// Warning `index`, valid until `doc` is freed; NULL when `doc` is NULL or `index` is out of range.
///
/// # Safety
/// `doc` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_document_warning(doc: *const shadewire_document, index: usize) -> *const c_char {
    match unsafe { doc.as_ref() }.and_then(|doc| doc.warnings.get(index)) {
        Some(warning) => warning.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Frees `doc`; NULL is allowed.
///
/// # Safety
/// `doc` must be NULL or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_document_free(doc: *mut shadewire_document) {
    if !doc.is_null() {
        drop(unsafe { Box::from_raw(doc) });
    }
}
