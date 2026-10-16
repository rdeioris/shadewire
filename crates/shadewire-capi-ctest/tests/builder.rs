//! Runs the C tests of the graph builder (tests/builder.c).

use std::ffi::CStr;
use std::os::raw::c_char;

use shadewire_capi_ctest::{
    shadewire_ctest_builder_error_names_source_path, shadewire_ctest_builder_matches_xml,
    shadewire_ctest_builder_rejects_null_and_bad_input, shadewire_ctest_builder_to_ir_twice,
};

fn run(test: unsafe extern "C" fn(*mut c_char, usize) -> i32) {
    let mut report = vec![0 as c_char; 512];
    let step = unsafe { test(report.as_mut_ptr(), report.len()) };
    let message = unsafe { CStr::from_ptr(report.as_ptr()) }.to_string_lossy().into_owned();
    assert_eq!(step, 0, "step {step}: {message}");
}

#[test]
fn builder_matches_xml() {
    run(shadewire_ctest_builder_matches_xml);
}

#[test]
fn builder_error_names_source_path() {
    run(shadewire_ctest_builder_error_names_source_path);
}

#[test]
fn builder_rejects_null_and_bad_input() {
    run(shadewire_ctest_builder_rejects_null_and_bad_input);
}

#[test]
fn builder_to_ir_twice() {
    run(shadewire_ctest_builder_to_ir_twice);
}
