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

// cbindgen copies only literal constants into the header, so the version is spelled out here and checked against
// Cargo.toml below: a version bump that forgets these lines does not compile.

/// The major version of the header; compare with `shadewire_version() >> 16` to check the library.
pub const SHADEWIRE_VERSION_MAJOR: u32 = 0;
/// The minor version of the header.
pub const SHADEWIRE_VERSION_MINOR: u32 = 2;
/// The patch version of the header.
pub const SHADEWIRE_VERSION_PATCH: u32 = 0;

const _: () = assert!(SHADEWIRE_VERSION_MAJOR == component(env!("CARGO_PKG_VERSION_MAJOR")), "update the version");
const _: () = assert!(SHADEWIRE_VERSION_MINOR == component(env!("CARGO_PKG_VERSION_MINOR")), "update the version");
const _: () = assert!(SHADEWIRE_VERSION_PATCH == component(env!("CARGO_PKG_VERSION_PATCH")), "update the version");
static VERSION_STRING: &str = concat!(env!("CARGO_PKG_VERSION"), "\0");

/// The version as `(major << 16) | (minor << 8) | patch`.
#[unsafe(no_mangle)]
pub extern "C" fn shadewire_version() -> u32 {
    (SHADEWIRE_VERSION_MAJOR << 16) | (SHADEWIRE_VERSION_MINOR << 8) | SHADEWIRE_VERSION_PATCH
}

/// The version as a static NUL-terminated string, e.g. "0.2.0".
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
        assert_eq!(shadewire_version(), 2 << 8);
    }

    #[test]
    fn the_header_constants_match_shadewire_version() {
        let packed = (SHADEWIRE_VERSION_MAJOR << 16) | (SHADEWIRE_VERSION_MINOR << 8) | SHADEWIRE_VERSION_PATCH;
        assert_eq!(packed, shadewire_version());
        let text = format!("{SHADEWIRE_VERSION_MAJOR}.{SHADEWIRE_VERSION_MINOR}.{SHADEWIRE_VERSION_PATCH}");
        assert_eq!(text, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_string_reads_the_package_version() {
        let text = unsafe { CStr::from_ptr(shadewire_version_string()) };
        assert_eq!(text.to_str().unwrap(), "0.2.0");
    }
}
