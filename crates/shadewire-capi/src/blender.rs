//! The Blender graph front end (Python's `shadewire.blender`): `shadewire_blender_to_ir`, `shadewire_blender_material_names`.

use std::os::raw::c_char;
use std::panic::AssertUnwindSafe;

use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::ir::shadewire_ir;
use crate::names::shadewire_strings;
use crate::options::{read_ir_options, shadewire_ir_options};
use crate::status::shadewire_status;
use crate::strings::{opt_cstr, text_arg};

/// Lowers a Blender material graph document `text` (`len` bytes of UTF-8, not NUL-terminated; the JSON the Blender
/// extension's File > Export > Material Graph writes) to the IR and writes the handle to `out`. `material` selects a
/// material by name (NULL for the document's only one: SHADEWIRE_ERROR_MATERIAL when it has several); `options` may
/// be NULL for the defaults. The IR is not optimized. The front end's warnings are read with `shadewire_ir_warnings`.
/// `*out` is written only on success.
///
/// # Safety
/// As `shadewire_mtlx_to_ir`, with `options` NULL or valid (see `shadewire_ir_options`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_blender_to_ir(
    text: *const c_char,
    len: usize,
    material: *const c_char,
    options: *const shadewire_ir_options,
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
                let material = opt_cstr(material)?;
                let options = read_ir_options(options)?;
                let ir = shadewire_blender::to_ir(text, material, &options)?;
                *out = Box::into_raw(Box::new(shadewire_ir::new(ir)));
                Ok(())
            }),
        )
    }
}

/// The names of a Blender graph document's materials, in document order, written to `out`. `text` and `len` as
/// `shadewire_blender_to_ir`. `*out` is written only on success.
///
/// # Safety
/// `text` must hold `len` readable bytes (NULL is reported, not dereferenced); `out` must be valid for writing a
/// pointer and `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_blender_material_names(
    text: *const c_char,
    len: usize,
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
                let names = shadewire_blender::material_names(text)?;
                *out = Box::into_raw(Box::new(shadewire_strings::new(names)));
                Ok(())
            }),
        )
    }
}
