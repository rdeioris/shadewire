//! Linking a host prelude and translating the result to another shading language.

use std::panic::AssertUnwindSafe;

use shadewire_wgsl::{Output, Target, link, reference_host, translate};

use crate::blob::shadewire_blob;
use crate::compiled::{borrow, shadewire_compiled};
use crate::error::{CapiError, shadewire_error};
use crate::guard::guarded;
use crate::status::shadewire_status;
use crate::strings::opt_cstr;
use crate::target::{shadewire_target, to_target};

/// Links `host_prelude` (WGSL defining every `shadewire_host_*` function; NULL for the reference prelude, which backs
/// each read with a private variable) to the compiled material and translates the result to `target`.
///
/// WGSL targets yield the linked text; HLSL, GLSL and MSL yield text blobs; SPIR-V yields a binary blob. Prelude
/// problems come back as SHADEWIRE_ERROR_SHADER errors. `*out` is written only on success.
///
/// # Safety
/// `compiled` must be a live handle (NULL is an error); `host_prelude` NULL or a valid NUL-terminated string; `out`
/// valid for writing a pointer; `error` NULL or valid for writing a pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_translate(
    compiled: *const shadewire_compiled,
    host_prelude: *const std::os::raw::c_char,
    target: shadewire_target,
    out: *mut *mut shadewire_blob,
    error: *mut *mut shadewire_error,
) -> shadewire_status {
    unsafe {
        guarded(
            error,
            AssertUnwindSafe(|| {
                if out.is_null() {
                    return Err(CapiError::Argument("out is NULL".to_string()));
                }
                let handle = borrow(compiled)?;
                let prelude = opt_cstr(host_prelude)?;
                let target = to_target(target)?;
                let blob = run(handle, prelude, target)?;
                *out = Box::into_raw(Box::new(blob));
                Ok(())
            }),
        )
    }
}

fn run(handle: &shadewire_compiled, prelude: Option<&str>, target: Target) -> Result<shadewire_blob, CapiError> {
    let compiled = &handle.compiled;
    let prelude = match prelude {
        Some(text) => text.to_string(),
        None => reference_host::prelude(&compiled.reflection, &handle.struct_name()),
    };
    let linked = link(&compiled.wgsl, &prelude)?;
    if target == Target::Wgsl {
        return Ok(shadewire_blob::text(linked));
    }
    Ok(match translate(&linked, target, None)? {
        Output::Text(text) => shadewire_blob::text(text),
        Output::Words(words) => shadewire_blob::words(&words),
    })
}
