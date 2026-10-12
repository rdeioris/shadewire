//! Library version.

use std::os::raw::c_char;

const fn component(text: &str) -> u32 {
    let bytes = text.as_bytes();
    let mut value = 0u32;
    let mut i = 0;
    while i < bytes.len() {
        value = value * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    value
}

const MAJOR: u32 = component(env!("CARGO_PKG_VERSION_MAJOR"));
const MINOR: u32 = component(env!("CARGO_PKG_VERSION_MINOR"));
const PATCH: u32 = component(env!("CARGO_PKG_VERSION_PATCH"));
static VERSION_STRING: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");

/// The version as `(major << 16) | (minor << 8) | patch`.
#[unsafe(no_mangle)]
pub extern "C" fn shadewire_version() -> u32 {
    (MAJOR << 16) | (MINOR << 8) | PATCH
}

/// The version as a static NUL-terminated string, e.g. "0.1.0".
#[unsafe(no_mangle)]
pub extern "C" fn shadewire_version_string() -> *const c_char {
    VERSION_STRING.as_ptr().cast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn version_packs_major_minor_patch() {
        assert_eq!(shadewire_version(), 1 << 8);
    }

    #[test]
    fn version_string_reads_the_package_version() {
        let text = unsafe { CStr::from_ptr(shadewire_version_string()) };
        assert_eq!(text.to_str().unwrap(), "0.1.0");
    }
}
