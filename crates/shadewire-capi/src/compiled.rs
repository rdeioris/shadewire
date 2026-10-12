//! The back end: the IR to WGSL and reflection, as the opaque `shadewire_compiled` handle.

use std::ffi::CString;
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;

use shadewire::model_struct_name;
use shadewire_wgsl::{CompileOptions, Compiled};

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::{borrow as borrow_ir, lower, shadewire_ir};
use crate::options::{read_compile, shadewire_compile_options, shadewire_mtlx_options};
use crate::status::shadewire_status;
use crate::strings::{ArgError, c_string, cached_text, null_str, out_str};

/// A compiled material: WGSL, reflection and the optimized IR. Free it with `shadewire_compiled_free`.
pub struct shadewire_compiled {
    pub(crate) compiled: Compiled,
    wgsl: CString,
    reflection_json: CString,
    struct_name: CString,
    dump: OnceLock<CString>,
}

/// Borrows the compiled material behind `ptr`.
///
/// # Safety
/// `ptr` must be NULL or a live handle that outlives `'a`.
pub(crate) unsafe fn borrow<'a>(ptr: *const shadewire_compiled) -> Result<&'a shadewire_compiled, ArgError> {
    unsafe { ptr.as_ref() }.ok_or_else(|| ArgError("compiled is NULL".to_string()))
}

impl shadewire_compiled {
    /// The WGSL struct name of the shading model.
    pub(crate) fn struct_name(&self) -> String {
        self.struct_name.to_string_lossy().into_owned()
    }
}

fn build(
    dag: &shadewire::Dag,
    source: &shadewire::SourceInfo,
    options: &CompileOptions,
) -> Result<shadewire_compiled, CapiError> {
    let compiled = shadewire_wgsl::compile(dag, source, options)?;
    let json = serde_json::to_string_pretty(&compiled.reflection)
        .map_err(|e| CapiError::Argument(format!("reflection could not be serialised: {e}")))?;
    Ok(shadewire_compiled {
        wgsl: c_string(compiled.wgsl.clone()),
        reflection_json: c_string(json),
        struct_name: c_string(model_struct_name(&compiled.reflection.model)),
        dump: OnceLock::new(),
        compiled,
    })
}

/// Compiles `ir` to WGSL and reflection and writes the handle to `out`. `options` may be NULL for the defaults.
/// `*out` is written only on success.
///
/// # Safety
/// `ir` must be a live handle (NULL is an error); `options` NULL or valid (see `shadewire_compile_options`); `out`
/// valid for writing a pointer; `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_wgsl_compile(
    ir: *const shadewire_ir,
    options: *const shadewire_compile_options,
    out: *mut *mut shadewire_compiled,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let ir = borrow_ir(ir)?;
                let options = read_compile(options)?;
                *out = Box::into_raw(Box::new(build(&ir.dag, &ir.source, &options)?));
                Ok(())
            }),
        )
    }
}

/// `shadewire_mtlx_to_ir` then `shadewire_wgsl_compile` in one call, with one handle out. `text` is `len` bytes of
/// UTF-8 and must be non-NULL (NULL is SHADEWIRE_ERROR_INVALID_ARGUMENT); `mtlx` and `options` may be NULL for the
/// defaults. `*out` is written only on success.
///
/// # Safety
/// The union of the safety requirements of `shadewire_mtlx_to_ir` and `shadewire_wgsl_compile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compile_mtlx(
    text: *const c_char,
    len: usize,
    path: *const c_char,
    material: *const c_char,
    mtlx: *const shadewire_mtlx_options,
    options: *const shadewire_compile_options,
    out: *mut *mut shadewire_compiled,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let ir = lower(text, len, path, material, mtlx)?;
                let options = read_compile(options)?;
                *out = Box::into_raw(Box::new(build(&ir.ir.dag, &ir.ir.source, &options)?));
                Ok(())
            }),
        )
    }
}

/// The WGSL source, valid until `c` is freed; writes its byte length to `len` when non-null. NULL when `c` is NULL.
///
/// # Safety
/// `c` must be NULL or a live handle; `len` NULL or valid for writing a `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_wgsl(c: *const shadewire_compiled, len: *mut usize) -> *const c_char {
    unsafe { text_of(c, len, |c| &c.wgsl) }
}

/// The reflection as pretty-printed JSON, valid until `c` is freed; see `shadewire_compiled_wgsl` for `len`.
///
/// # Safety
/// As `shadewire_compiled_wgsl`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_reflection_json(
    c: *const shadewire_compiled,
    len: *mut usize,
) -> *const c_char {
    unsafe { text_of(c, len, |c| &c.reflection_json) }
}

/// A text dump of the optimized IR, built on first call and valid until `c` is freed; see
/// `shadewire_compiled_wgsl` for `len`, which is 0 on failure. NULL when `c` is NULL or building the dump failed.
///
/// # Safety
/// As `shadewire_compiled_wgsl`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_ir_dump(c: *const shadewire_compiled, len: *mut usize) -> *const c_char {
    match unsafe { c.as_ref() } {
        Some(handle) => match cached_text(&handle.dump, || handle.compiled.dag.dump()) {
            Some(dump) => unsafe { out_str(dump, len) },
            None => unsafe { null_str(len) },
        },
        None => unsafe { null_str(len) },
    }
}

/// The WGSL struct name of the shading model (for example "StandardSurface"), valid until `c` is freed; NULL when `c`
/// is NULL.
///
/// # Safety
/// `c` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_struct_name(c: *const shadewire_compiled) -> *const c_char {
    unsafe { text_of(c, std::ptr::null_mut(), |c| &c.struct_name) }
}

/// Frees `c`; NULL is allowed.
///
/// # Safety
/// `c` must be NULL or a handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_free(c: *mut shadewire_compiled) {
    if !c.is_null() {
        drop(unsafe { Box::from_raw(c) });
    }
}

/// The text `pick` selects from `c`, with its length written to `len`; NULL (and `len` 0) when `c` is NULL.
///
/// # Safety
/// `c` must be NULL or a live handle; `len` NULL or valid for writing a `size_t`.
unsafe fn text_of(
    c: *const shadewire_compiled,
    len: *mut usize,
    pick: impl for<'a> FnOnce(&'a shadewire_compiled) -> &'a CString,
) -> *const c_char {
    match unsafe { c.as_ref() } {
        Some(handle) => unsafe { out_str(pick(handle), len) },
        None => unsafe { null_str(len) },
    }
}
