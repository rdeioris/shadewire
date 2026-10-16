//! String conversions at the C boundary.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

/// An invalid argument passed by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArgError(pub(crate) String);

/// Makes a C string, replacing NUL bytes with U+FFFD.
pub(crate) fn c_string(s: String) -> CString {
    let cleaned = if s.contains('\0') { s.replace('\0', "\u{FFFD}") } else { s };
    CString::new(cleaned).expect("NUL bytes were replaced")
}

/// The text cached in `cell`, built by `build` on first use; `None` when `build` panicked (nothing is cached then).
pub(crate) fn cached_text(cell: &OnceLock<CString>, build: impl FnOnce() -> String) -> Option<&CString> {
    if let Some(done) = cell.get() {
        return Some(done);
    }
    let built = catch_unwind(AssertUnwindSafe(|| c_string(build()))).ok()?;
    Some(cell.get_or_init(|| built))
}

/// The text cached in `cell` (built by `build` on first use), its byte length written to `len` when non-null; NULL
/// (and 0) when `build` panicked.
///
/// # Safety
/// `len` must be NULL or valid for writing a `usize`.
pub(crate) unsafe fn cached_out(
    cell: &OnceLock<CString>,
    build: impl FnOnce() -> String,
    len: *mut usize,
) -> *const c_char {
    match cached_text(cell, build) {
        Some(text) => unsafe { out_str(text, len) },
        None => unsafe { null_str(len) },
    }
}

/// Writes 0 to `len` when non-null and returns NULL.
///
/// # Safety
/// `len` must be NULL or valid for writing a `usize`.
pub(crate) unsafe fn null_str(len: *mut usize) -> *const c_char {
    if !len.is_null() {
        unsafe { *len = 0 };
    }
    std::ptr::null()
}

/// Returns the pointer of `s`, writing its byte length to `len` when non-null.
///
/// # Safety
/// `len` must be NULL or valid for writing a `usize`.
pub(crate) unsafe fn out_str(s: &CStr, len: *mut usize) -> *const c_char {
    if !len.is_null() {
        unsafe { *len = s.to_bytes().len() };
    }
    s.as_ptr()
}

/// Reads a UTF-8 argument of `len` bytes at `ptr`; NULL is an error.
///
/// # Safety
/// `ptr` must be NULL or point to `len` readable bytes that outlive `'a`.
pub(crate) unsafe fn text_arg<'a>(ptr: *const c_char, len: usize) -> Result<&'a str, ArgError> {
    if ptr.is_null() {
        return Err(ArgError("null text pointer".to_string()));
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes).map_err(|_| ArgError("text is not valid UTF-8".to_string()))
}

/// Reads an optional NUL-terminated UTF-8 argument.
///
/// # Safety
/// `ptr` must be NULL or a valid NUL-terminated string that outlives `'a`.
pub(crate) unsafe fn opt_cstr<'a>(ptr: *const c_char) -> Result<Option<&'a str>, ArgError> {
    if ptr.is_null() {
        return Ok(None);
    }
    let text = unsafe { CStr::from_ptr(ptr) };
    text.to_str().map(Some).map_err(|_| ArgError("text is not valid UTF-8".to_string()))
}

/// Reads `count` NUL-terminated UTF-8 strings at `ptr`, naming each `what` in errors: NULL with a count of 0 is the
/// empty list, NULL with a count or a NULL entry an error.
///
/// # Safety
/// `ptr` must be NULL or hold `count` pointers, each NULL or a valid NUL-terminated string that outlives `'a`.
pub(crate) unsafe fn string_list<'a>(
    ptr: *const *const c_char,
    count: usize,
    what: &str,
) -> Result<Vec<&'a str>, ArgError> {
    if ptr.is_null() {
        return if count == 0 {
            Ok(Vec::new())
        } else {
            Err(ArgError(format!("{what}s is NULL but the count is {count}")))
        };
    }
    (0..count)
        .map(|i| unsafe { opt_cstr(*ptr.add(i)) }?.ok_or_else(|| ArgError(format!("{what} {i} is NULL"))))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_string_has_no_interior_nul() {
        let s = c_string("a\0b".to_string());
        assert_eq!(s.to_str().unwrap(), "a\u{FFFD}b");
    }

    #[test]
    fn text_arg_rejects_null() {
        assert!(unsafe { text_arg(std::ptr::null(), 0) }.is_err());
    }

    #[test]
    fn text_arg_rejects_invalid_utf8() {
        let bytes = [0xffu8, 0xfe];
        assert!(unsafe { text_arg(bytes.as_ptr().cast(), 2) }.is_err());
    }

    #[test]
    fn text_arg_reads_valid_text() {
        let text = "héllo";
        assert_eq!(unsafe { text_arg(text.as_ptr().cast(), text.len()) }.unwrap(), text);
    }

    #[test]
    fn opt_cstr_null_is_none() {
        assert_eq!(unsafe { opt_cstr(std::ptr::null()) }, Ok(None));
    }

    #[test]
    fn opt_cstr_reads_text_and_rejects_invalid_utf8() {
        let good = CString::new("abc").unwrap();
        assert_eq!(unsafe { opt_cstr(good.as_ptr()) }, Ok(Some("abc")));
        let bad = CString::new(vec![0xffu8]).unwrap();
        assert!(unsafe { opt_cstr(bad.as_ptr()) }.is_err());
    }

    #[test]
    fn out_str_writes_the_byte_length() {
        let s = CString::new("héllo").unwrap();
        let mut len = 0usize;
        let ptr = unsafe { out_str(&s, &mut len) };
        assert_eq!(ptr, s.as_ptr());
        assert_eq!(len, 6);
        assert_eq!(unsafe { out_str(&s, std::ptr::null_mut()) }, s.as_ptr());
    }
}
