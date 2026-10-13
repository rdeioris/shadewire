//! The back ends (Python's `shadewire.wgsl`, `.glsl`, `.hlsl`, `.msl`): the IR to code and reflection, as the opaque
//! `shadewire_compiled` handle (Python's `shadewire.Compiled`).

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;
use std::sync::OnceLock;

use shadewire::model_struct_name;
use shadewire::shader::{CompileOptions, Compiled};

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::{borrow as borrow_ir, shadewire_ir};
use crate::options::{read_compile, shadewire_compile_options};
use crate::status::shadewire_status;
use crate::strings::{c_string, cached_out, null_str, out_str};
use crate::target::{Backend, backend, shadewire_target};

/// A compiled material: the code in its language (the model's struct, `shadewire_eval_surface()`, the closure
/// functions, declarations of the host functions it calls), its reflection, and the IR the code was printed from
/// (optimized when the options asked for it). Free it with `shadewire_compiled_free`.
pub struct shadewire_compiled {
    pub(crate) compiled: Compiled,
    language: &'static CStr,
    code: CString,
    reflection_json: CString,
    model_struct: CString,
    dump: OnceLock<CString>,
}

fn build(
    dag: &shadewire::Dag,
    source: &shadewire::SourceInfo,
    options: &CompileOptions,
    target: Backend,
) -> Result<shadewire_compiled, CapiError> {
    let (compiled, language) = match target {
        Backend::Wgsl => (shadewire_wgsl::compile(dag, source, options)?, c"wgsl"),
        Backend::Glsl(version) => (shadewire_glsl::compile(dag, source, options, version)?, c"glsl"),
        Backend::Hlsl => (shadewire_hlsl::compile(dag, source, options)?, c"hlsl"),
        Backend::Msl => (shadewire_msl::compile(dag, source, options)?, c"msl"),
    };
    let json = serde_json::to_string_pretty(&compiled.reflection)
        .map_err(|e| CapiError::Internal(format!("reflection could not be serialised: {e}")))?;
    Ok(shadewire_compiled {
        language,
        code: c_string(compiled.code.clone()),
        reflection_json: c_string(json),
        model_struct: c_string(model_struct_name(&compiled.reflection.model)),
        dump: OnceLock::new(),
        compiled,
    })
}

/// Compiles `ir` to the code of `target` and its reflection and writes the handle to `out`. `options` may be NULL for
/// the defaults. `*out` is written only on success.
///
/// # Safety
/// `ir` must be a live handle (NULL is an error); `options` NULL or valid (see `shadewire_compile_options`); `out`
/// valid for writing a pointer; `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compile(
    ir: *const shadewire_ir,
    target: shadewire_target,
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
                let target = backend(target)?;
                let options = read_compile(options)?;
                *out = Box::into_raw(Box::new(build(&ir.dag, &ir.source, &options, target)?));
                Ok(())
            }),
        )
    }
}

/// The language of the code: "wgsl", "glsl" (GLSL ES too), "hlsl" or "msl"; static. NULL when `c` is NULL.
///
/// # Safety
/// `c` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_language(c: *const shadewire_compiled) -> *const c_char {
    match unsafe { c.as_ref() } {
        Some(handle) => handle.language.as_ptr(),
        None => std::ptr::null(),
    }
}

/// The generated code, valid until `c` is freed; writes its byte length to `len` when non-null. NULL when `c` is
/// NULL.
///
/// # Safety
/// `c` must be NULL or a live handle; `len` NULL or valid for writing a `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_code(c: *const shadewire_compiled, len: *mut usize) -> *const c_char {
    unsafe { text_of(c, len, |c| &c.code) }
}

/// The reflection as pretty-printed JSON (bindings, uniforms with offsets and defaults, textures, the host functions
/// the code calls, each field's kind, the closure tree of a `surface` material), valid until `c` is freed; see
/// `shadewire_compiled_code` for `len`.
///
/// # Safety
/// As `shadewire_compiled_code`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_reflection_json(
    c: *const shadewire_compiled,
    len: *mut usize,
) -> *const c_char {
    unsafe { text_of(c, len, |c| &c.reflection_json) }
}

/// A text dump of the optimized IR, built on first call and valid until `c` is freed; see
/// `shadewire_compiled_code` for `len`, which is 0 on failure. NULL when `c` is NULL or building the dump failed.
///
/// # Safety
/// As `shadewire_compiled_code`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_ir_dump(c: *const shadewire_compiled, len: *mut usize) -> *const c_char {
    match unsafe { c.as_ref() } {
        Some(handle) => unsafe { cached_out(&handle.dump, || handle.compiled.dag.dump(), len) },
        None => unsafe { null_str(len) },
    }
}

/// The name of the struct `shadewire_eval_surface` returns (for example "StandardSurface"), valid until `c` is freed;
/// NULL when `c` is NULL.
///
/// # Safety
/// `c` must be NULL or a live handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_compiled_model_struct(c: *const shadewire_compiled) -> *const c_char {
    unsafe { text_of(c, std::ptr::null_mut(), |c| &c.model_struct) }
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
