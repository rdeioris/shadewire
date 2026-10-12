//! The C smoke test of the Shadewire C API, compiled by build.rs and linked here.

use std::os::raw::c_char;

pub use shadewire_capi;

unsafe extern "C" {
    /// Runs the smoke test on the MaterialX file at `mtlx_path`; 0 on success, otherwise the failed step's number
    /// with a message in `report` (`report_len` bytes, NUL-terminated).
    pub fn shadewire_smoke(mtlx_path: *const c_char, report: *mut c_char, report_len: usize) -> i32;
}

/// The library version, through the C API, so the Rust side of the archive stays linked.
pub fn version() -> u32 {
    shadewire_capi::shadewire_version()
}
