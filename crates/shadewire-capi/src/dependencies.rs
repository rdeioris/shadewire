//! What a document reads besides itself (`shadewire_mtlx::list_dependencies`), as the opaque `shadewire_dependencies`
//! handle, so hosts with their own I/O can fetch everything and then compile with an in-memory resolver.

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::status::shadewire_status;
use crate::strings::{c_string, opt_cstr, text_arg};

/// The locations one document names, unresolved, in document order without repeats: its `xi:include` hrefs and its
/// texture files (`filename` values with `fileprefix` applied, as reflection reports them). Free it with
/// `shadewire_dependencies_free`.
pub struct shadewire_dependencies {
    includes: Vec<CString>,
    files: Vec<CString>,
}

/// The includes and files MaterialX `text` names, written to `out`. Includes are not followed: an included document
/// has its own, so list it again once fetched. `text`, `len` and `location` as `shadewire_mtlx_to_ir` (`location`
/// only names the document in errors). `*out` is written only on success.
///
/// # Safety
/// `text` must hold `len` readable bytes (NULL is reported, not dereferenced); `location` must be NULL or a valid
/// NUL-terminated string; `out` must be valid for writing a pointer and `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_mtlx_list_dependencies(
    text: *const c_char,
    len: usize,
    location: *const c_char,
    out: *mut *mut shadewire_dependencies,
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
                let deps = shadewire_mtlx::list_dependencies(text, location)?;
                *out = Box::into_raw(Box::new(shadewire_dependencies {
                    includes: deps.includes.into_iter().map(c_string).collect(),
                    files: deps.files.into_iter().map(c_string).collect(),
                }));
                Ok(())
            }),
        )
    }
}

fn item(list: Option<&[CString]>, index: usize) -> *const c_char {
    list.and_then(|l| l.get(index)).map_or(std::ptr::null(), |s| s.as_ptr())
}

/// The number of includes; 0 when `d` is NULL.
///
/// # Safety
/// `d` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_dependencies_include_count(d: *const shadewire_dependencies) -> usize {
    unsafe { d.as_ref() }.map_or(0, |d| d.includes.len())
}

/// Include `index` (an `href` as written, relative to the document), valid until `d` is freed; NULL when `d` is NULL
/// or `index` is out of range.
///
/// # Safety
/// `d` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_dependencies_include(
    d: *const shadewire_dependencies,
    index: usize,
) -> *const c_char {
    item(unsafe { d.as_ref() }.map(|d| d.includes.as_slice()), index)
}

/// The number of files; 0 when `d` is NULL.
///
/// # Safety
/// `d` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_dependencies_file_count(d: *const shadewire_dependencies) -> usize {
    unsafe { d.as_ref() }.map_or(0, |d| d.files.len())
}

/// File `index` (relative to the document, `fileprefix` applied), valid until `d` is freed; NULL when `d` is NULL or
/// `index` is out of range.
///
/// # Safety
/// `d` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_dependencies_file(d: *const shadewire_dependencies, index: usize) -> *const c_char {
    item(unsafe { d.as_ref() }.map(|d| d.files.as_slice()), index)
}

/// Frees `d`; NULL is allowed.
///
/// # Safety
/// `d` must be NULL or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_dependencies_free(d: *mut shadewire_dependencies) {
    if !d.is_null() {
        drop(unsafe { Box::from_raw(d) });
    }
}
