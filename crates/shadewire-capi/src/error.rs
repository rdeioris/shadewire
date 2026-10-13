//! Error objects: the opaque `shadewire_error` handle and its accessors.

use std::any::Any;
use std::ffi::CString;
use std::os::raw::c_char;

use crate::error_kind::{kind_of, shadewire_error_kind};
use crate::strings::{ArgError, c_string, out_str};

/// An error returned by a failed call. Free it with `shadewire_error_free`.
pub struct shadewire_error {
    kind: shadewire_error_kind,
    message: CString,
    file: Option<CString>,
    line: u32,
    path: CString,
}

/// Everything a C API body can fail with.
#[derive(Debug)]
pub(crate) enum CapiError {
    /// An error from the core library.
    Core(shadewire::Error),
    /// An invalid argument passed by the caller.
    Argument(String),
    /// A fault inside the library that the caller cannot fix; reported as SHADEWIRE_ERROR_PANIC.
    Internal(String),
}

impl From<shadewire::Error> for CapiError {
    fn from(error: shadewire::Error) -> Self {
        CapiError::Core(error)
    }
}

impl From<ArgError> for CapiError {
    fn from(error: ArgError) -> Self {
        CapiError::Argument(error.0)
    }
}

impl From<CapiError> for shadewire_error {
    fn from(error: CapiError) -> Self {
        match error {
            CapiError::Core(core) => from_core(&core),
            CapiError::Argument(message) => invalid_argument(message),
            CapiError::Internal(message) => unlocated(shadewire_error_kind::SHADEWIRE_ERROR_PANIC, message),
        }
    }
}

/// The location carried by a core error, if its variant has one.
fn location_of(error: &shadewire::Error) -> Option<&shadewire::Location> {
    use shadewire::Error;
    match error {
        Error::Parse { at, .. }
        | Error::UnsupportedVersion { at, .. }
        | Error::UnknownNode { at, .. }
        | Error::NoMatchingNodeDef { at, .. }
        | Error::TypeMismatch { at, .. }
        | Error::UnsupportedType { at, .. }
        | Error::BadValue { at, .. }
        | Error::Cycle { at, .. }
        | Error::MissingLeafImpl { at, .. }
        | Error::UnsupportedSurface { at, .. }
        | Error::UnsupportedColorspace { at, .. } => Some(at),
        Error::NoTranslation { .. }
        | Error::NoMaterial
        | Error::AmbiguousMaterial { .. }
        | Error::UnknownMaterial { .. }
        | Error::Io(_)
        | Error::UnsupportedTarget(_) => None,
    }
}

/// Builds an error object from a core error.
pub(crate) fn from_core(error: &shadewire::Error) -> shadewire_error {
    let at = location_of(error);
    shadewire_error {
        kind: kind_of(error),
        message: c_string(error.to_string()),
        file: at.and_then(|l| l.file.clone()).map(c_string),
        line: at.map_or(0, |l| l.line),
        path: c_string(at.map_or_else(String::new, |l| l.path.clone())),
    }
}

fn unlocated(kind: shadewire_error_kind, message: String) -> shadewire_error {
    shadewire_error { kind, message: c_string(message), file: None, line: 0, path: c_string(String::new()) }
}

/// Builds an invalid-argument error object.
pub(crate) fn invalid_argument(message: String) -> shadewire_error {
    unlocated(shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT, message)
}

/// Builds a panic error object from a caught panic payload.
pub(crate) fn panic(payload: Box<dyn Any + Send>) -> shadewire_error {
    let message = match payload.downcast::<&'static str>() {
        Ok(text) => text.to_string(),
        Err(payload) => match payload.downcast::<String>() {
            Ok(text) => *text,
            Err(_) => "panic".to_string(),
        },
    };
    unlocated(shadewire_error_kind::SHADEWIRE_ERROR_PANIC, message)
}

/// The fixed error written when building an error object itself panicked.
pub(crate) fn reporting_panic() -> shadewire_error {
    unlocated(shadewire_error_kind::SHADEWIRE_ERROR_PANIC, "panic while reporting an error".to_string())
}

/// The kind of `err`; `SHADEWIRE_ERROR_INVALID_ARGUMENT` when `err` is NULL.
///
/// # Safety
/// `err` must be NULL or a live error handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_error_get_kind(err: *const shadewire_error) -> shadewire_error_kind {
    match unsafe { err.as_ref() } {
        Some(e) => e.kind,
        None => shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT,
    }
}

/// The message of `err` as a NUL-terminated string valid until `err` is freed; writes its byte length to `len` when
/// non-null. NULL when `err` is NULL (`len` is then 0).
///
/// # Safety
/// `err` must be NULL or a live error handle; `len` NULL or valid for writing a `size_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_error_message(err: *const shadewire_error, len: *mut usize) -> *const c_char {
    match unsafe { err.as_ref() } {
        Some(e) => unsafe { out_str(&e.message, len) },
        None => {
            if !len.is_null() {
                unsafe { *len = 0 };
            }
            std::ptr::null()
        }
    }
}

/// The document file name of `err`, or NULL when it has none (or `err` is NULL); valid until `err` is freed.
///
/// # Safety
/// `err` must be NULL or a live error handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_error_file(err: *const shadewire_error) -> *const c_char {
    match unsafe { err.as_ref() }.and_then(|e| e.file.as_ref()) {
        Some(file) => file.as_ptr(),
        None => std::ptr::null(),
    }
}

/// The 1-based document line of `err`, or 0 when it has none (or `err` is NULL).
///
/// # Safety
/// `err` must be NULL or a live error handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_error_line(err: *const shadewire_error) -> u32 {
    unsafe { err.as_ref() }.map_or(0, |e| e.line)
}

/// The node path of `err`, "" when it has none; NULL when `err` is NULL; valid until `err` is freed.
///
/// # Safety
/// `err` must be NULL or a live error handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_error_path(err: *const shadewire_error) -> *const c_char {
    match unsafe { err.as_ref() } {
        Some(e) => e.path.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Frees `err`; NULL is allowed.
///
/// # Safety
/// `err` must be NULL or an error handle from this library not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn shadewire_error_free(err: *mut shadewire_error) {
    if !err.is_null() {
        drop(unsafe { Box::from_raw(err) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::guarded;
    use crate::shadewire_status;
    use shadewire::{Error, Location};
    use std::ffi::CStr;

    fn text(ptr: *const c_char) -> String {
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string()
    }

    fn fail_with(error: CapiError) -> *mut shadewire_error {
        let mut out: *mut shadewire_error = std::ptr::null_mut();
        let status = unsafe { guarded(&mut out, || Err(error)) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
        assert!(!out.is_null());
        out
    }

    #[test]
    fn a_located_core_error_carries_its_location() {
        let at = Location { file: Some("m.mtlx".into()), line: 7, path: "g/n".into() };
        let err = fail_with(Error::TypeMismatch { at, expected: "float".into(), found: "color3".into() }.into());
        unsafe {
            assert_eq!(shadewire_error_get_kind(err), shadewire_error_kind::SHADEWIRE_ERROR_GRAPH);
            assert_eq!(text(shadewire_error_file(err)), "m.mtlx");
            assert_eq!(shadewire_error_line(err), 7);
            assert_eq!(text(shadewire_error_path(err)), "g/n");
            let mut len = 0usize;
            let message = text(shadewire_error_message(err, &mut len));
            assert!(message.contains("type mismatch"), "{message}");
            assert_eq!(len, message.len());
            shadewire_error_free(err);
        }
    }

    #[test]
    fn an_unlocated_core_error_has_no_file_line_or_path() {
        let err = fail_with(Error::NoMaterial.into());
        unsafe {
            assert_eq!(shadewire_error_get_kind(err), shadewire_error_kind::SHADEWIRE_ERROR_MATERIAL);
            assert!(shadewire_error_file(err).is_null());
            assert_eq!(shadewire_error_line(err), 0);
            assert_eq!(text(shadewire_error_path(err)), "");
            shadewire_error_free(err);
        }
    }

    #[test]
    fn a_located_error_without_a_file_has_a_null_file() {
        let at = Location { file: None, line: 3, path: "p".into() };
        let err = fail_with(Error::Parse { at, message: "bad".into() }.into());
        unsafe {
            assert!(shadewire_error_file(err).is_null());
            assert_eq!(shadewire_error_line(err), 3);
            shadewire_error_free(err);
        }
    }

    #[test]
    fn an_argument_error_is_invalid_argument() {
        let err = fail_with(CapiError::Argument("bad arg".into()));
        unsafe {
            assert_eq!(shadewire_error_get_kind(err), shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
            assert_eq!(text(shadewire_error_message(err, std::ptr::null_mut())), "bad arg");
            shadewire_error_free(err);
        }
    }

    #[test]
    fn an_internal_fault_is_a_panic_error() {
        let err = fail_with(CapiError::Internal("reflection could not be serialised".into()));
        unsafe {
            assert_eq!(shadewire_error_get_kind(err), shadewire_error_kind::SHADEWIRE_ERROR_PANIC);
            assert_eq!(text(shadewire_error_message(err, std::ptr::null_mut())), "reflection could not be serialised");
            shadewire_error_free(err);
        }
    }

    #[test]
    fn a_panic_becomes_a_panic_error_with_its_message() {
        let mut out: *mut shadewire_error = std::ptr::null_mut();
        let status = unsafe { guarded(&mut out, || panic!("boom")) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
        unsafe {
            assert_eq!(shadewire_error_get_kind(out), shadewire_error_kind::SHADEWIRE_ERROR_PANIC);
            assert!(text(shadewire_error_message(out, std::ptr::null_mut())).contains("boom"));
            shadewire_error_free(out);
        }
    }

    #[test]
    fn location_of_returns_the_at_of_every_located_variant() {
        let at = || Location { file: Some("f.mtlx".into()), line: 4, path: "a/b".into() };
        let located = [
            Error::Parse { at: at(), message: "m".into() },
            Error::UnsupportedVersion { at: at(), version: "v".into() },
            Error::UnknownNode { at: at(), category: "c".into() },
            Error::NoMatchingNodeDef { at: at(), category: "c".into(), output: "o".into(), inputs: "i".into() },
            Error::TypeMismatch { at: at(), expected: "e".into(), found: "f".into() },
            Error::UnsupportedType { at: at(), ty: "t".into() },
            Error::BadValue { at: at(), message: "m".into() },
            Error::Cycle { at: at(), node: "n".into() },
            Error::MissingLeafImpl { at: at(), nodedef: "d".into() },
            Error::UnsupportedSurface { at: at(), node: "n".into(), reason: "r".into() },
            Error::UnsupportedColorspace { at: at(), colorspace: "c".into() },
        ];
        for e in &located {
            assert_eq!(location_of(e), Some(&at()), "{e}");
        }
        let unlocated = [Error::NoMaterial, Error::Io("x".into())];
        for e in &unlocated {
            assert_eq!(location_of(e), None, "{e}");
        }
    }

    #[test]
    fn the_reporting_fallback_is_a_panic_error() {
        let err = reporting_panic();
        assert_eq!(err.kind, shadewire_error_kind::SHADEWIRE_ERROR_PANIC);
        assert_eq!(err.message.to_str().unwrap(), "panic while reporting an error");
    }

    #[test]
    fn a_non_text_panic_payload_says_panic() {
        let err = panic(Box::new(42u32));
        assert_eq!(err.message.to_str().unwrap(), "panic");
    }

    #[test]
    fn a_null_error_out_parameter_drops_the_error() {
        let status = unsafe { guarded(std::ptr::null_mut(), || Err(CapiError::Argument("x".into()))) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
        let status = unsafe { guarded(std::ptr::null_mut(), || panic!("quiet")) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    }

    #[test]
    fn null_errors_are_harmless() {
        unsafe {
            shadewire_error_free(std::ptr::null_mut());
            let kind = shadewire_error_get_kind(std::ptr::null());
            assert_eq!(kind, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
            let mut len = 9usize;
            assert!(shadewire_error_message(std::ptr::null(), &mut len).is_null());
            assert_eq!(len, 0);
            assert!(shadewire_error_file(std::ptr::null()).is_null());
            assert_eq!(shadewire_error_line(std::ptr::null()), 0);
            assert!(shadewire_error_path(std::ptr::null()).is_null());
        }
    }
}
