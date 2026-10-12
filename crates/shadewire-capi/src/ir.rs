//! The front end: MaterialX text to the IR, as the opaque `shadewire_ir` handle.

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::path::Path;
use std::sync::OnceLock;

use shadewire_mtlx::MaterialIr;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::options::{read_mtlx, shadewire_mtlx_options};
use crate::status::shadewire_status;
use crate::strings::{ArgError, c_string, cached_text, null_str, opt_cstr, out_str, text_arg};

/// A MaterialX material lowered to the IR. Free it with `shadewire_ir_free`.
pub struct shadewire_ir {
    pub(crate) ir: MaterialIr,
    model: CString,
    dump: OnceLock<CString>,
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
/// `path` (NUL-terminated, may be NULL) names the document for messages and relative includes; `material` selects a
/// material by name (NULL for the first); `options` may be NULL for the defaults. `*out` is written only on success.
///
/// # Safety
/// `text` must hold `len` readable bytes (NULL is reported, not dereferenced); `path` and `material` must be NULL or
/// valid NUL-terminated strings; `options` must be NULL or valid (see `shadewire_mtlx_options`); `out` must be valid
/// for writing a pointer and `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mtlx_to_ir(
    text: *const c_char,
    len: usize,
    path: *const c_char,
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
                let handle = lower(text, len, path, material, options)?;
                *out = Box::into_raw(Box::new(handle));
                Ok(())
            }),
        )
    }
}

/// The shared body of `shadewire_mtlx_to_ir` and `shadewire_compile_mtlx`.
///
/// # Safety
/// As `shadewire_mtlx_to_ir`, except that NULL `text` is reported as an error.
pub(crate) unsafe fn lower(
    text: *const c_char,
    len: usize,
    path: *const c_char,
    material: *const c_char,
    options: *const shadewire_mtlx_options,
) -> Result<shadewire_ir, CapiError> {
    let text = unsafe { text_arg(text, len) }?;
    let path = unsafe { opt_cstr(path) }?;
    let material = unsafe { opt_cstr(material) }?;
    let options = unsafe { read_mtlx(options) }?;
    let ir = shadewire_mtlx::to_ir(text, path.map(Path::new), material, &options)?;
    let model = c_string(ir.dag.surface.model.clone());
    Ok(shadewire_ir { ir, model, dump: OnceLock::new() })
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

/// A text dump of the IR, built on first call and valid until `ir` is freed; writes its byte length to `len` when
/// non-null. NULL when `ir` is NULL or building the dump failed (len is then 0).
///
/// # Safety
/// `ir` must be NULL or a live handle; `len` NULL or valid for writing a `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_dump(ir: *const shadewire_ir, len: *mut usize) -> *const c_char {
    match unsafe { ir.as_ref() } {
        Some(handle) => match cached_text(&handle.dump, || handle.ir.dag.dump()) {
            Some(dump) => unsafe { out_str(dump, len) },
            None => unsafe { null_str(len) },
        },
        None => unsafe { null_str(len) },
    }
}

/// Frees `ir`; NULL is allowed.
///
/// # Safety
/// `ir` must be NULL or a handle from `shadewire_mtlx_to_ir` not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_ir_free(ir: *mut shadewire_ir) {
    if !ir.is_null() {
        drop(unsafe { Box::from_raw(ir) });
    }
}
