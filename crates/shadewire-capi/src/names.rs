//! A document's material names (Python's `shadewire.mtlx.material_names`), as the opaque `shadewire_strings` handle.

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::path::Path;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::status::shadewire_status;
use crate::strings::{c_string, opt_cstr, text_arg};

/// A list of strings. Free it with `shadewire_strings_free`.
pub struct shadewire_strings {
    items: Vec<CString>,
}

/// The names of a MaterialX document's materials, in document order, written to `out`. `text`, `len` and `path` as
/// `shadewire_mtlx_to_ir`. `*out` is written only on success.
///
/// # Safety
/// As `shadewire_mtlx_to_ir`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mtlx_material_names(
    text: *const c_char,
    len: usize,
    path: *const c_char,
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
                let text = text_arg(text, len)?;
                let path = opt_cstr(path)?;
                let names = shadewire_mtlx::material_names(text, path.map(Path::new))?;
                *out = Box::into_raw(Box::new(shadewire_strings { items: names.into_iter().map(c_string).collect() }));
                Ok(())
            }),
        )
    }
}

/// The number of strings; 0 when `s` is NULL.
///
/// # Safety
/// `s` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_strings_count(s: *const shadewire_strings) -> usize {
    unsafe { s.as_ref() }.map_or(0, |s| s.items.len())
}

/// String `index`, valid until `s` is freed; NULL when `s` is NULL or `index` is out of range.
///
/// # Safety
/// `s` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_strings_get(s: *const shadewire_strings, index: usize) -> *const c_char {
    match unsafe { s.as_ref() }.and_then(|s| s.items.get(index)) {
        Some(item) => item.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Frees `s`; NULL is allowed.
///
/// # Safety
/// `s` must be NULL or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_strings_free(s: *mut shadewire_strings) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
}
