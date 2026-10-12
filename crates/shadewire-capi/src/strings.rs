//! String conversions at the C boundary.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;

/// An invalid argument passed by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgError(pub String);

/// Makes a C string, replacing NUL bytes with U+FFFD.
pub fn c_string(s: String) -> CString {
    let cleaned = if s.contains('\0') { s.replace('\0', "\u{FFFD}") } else { s };
    CString::new(cleaned).expect("NUL bytes were replaced")
}

/// Returns the pointer of `s`, writing its byte length to `len` when non-null.
pub unsafe fn out_str(s: &CStr, len: *mut usize) -> *const c_char {
    if !len.is_null() {
        unsafe { *len = s.to_bytes().len() };
    }
    s.as_ptr()
}

/// Reads a UTF-8 argument of `len` bytes at `ptr`.
pub unsafe fn text_arg<'a>(ptr: *const c_char, len: usize) -> Result<&'a str, ArgError> {
    if ptr.is_null() {
        return Err(ArgError("null text pointer".to_string()));
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes).map_err(|_| ArgError("text is not valid UTF-8".to_string()))
}

/// Reads an optional NUL-terminated UTF-8 argument.
pub unsafe fn opt_cstr<'a>(ptr: *const c_char) -> Result<Option<&'a str>, ArgError> {
    if ptr.is_null() {
        return Ok(None);
    }
    let text = unsafe { CStr::from_ptr(ptr) };
    text.to_str().map(Some).map_err(|_| ArgError("text is not valid UTF-8".to_string()))
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
