//! The C smoke test of the Shadewire C API, compiled by build.rs and linked here.

use std::os::raw::c_char;

pub use shadewire_capi;

unsafe extern "C" {
    /// Runs the smoke test on the MaterialX file at `mtlx_path`; 0 on success, otherwise the failed step's number
    /// with a message in `report` (`report_len` bytes, NUL-terminated).
    pub fn shadewire_smoke(mtlx_path: *const c_char, report: *mut c_char, report_len: usize) -> i32;

    // The graph builder tests (tests/builder.c), with the same contract: 0, or the failed step and a message.
    pub fn shadewire_ctest_builder_matches_xml(report: *mut c_char, report_len: usize) -> i32;
    pub fn shadewire_ctest_builder_error_names_source_path(report: *mut c_char, report_len: usize) -> i32;
    pub fn shadewire_ctest_builder_rejects_null_and_bad_input(report: *mut c_char, report_len: usize) -> i32;
    pub fn shadewire_ctest_builder_to_ir_twice(report: *mut c_char, report_len: usize) -> i32;
}

/// The library version, through the C API, so the Rust side of the archive stays linked.
pub fn version() -> u32 {
    shadewire_capi::shadewire_version()
}
