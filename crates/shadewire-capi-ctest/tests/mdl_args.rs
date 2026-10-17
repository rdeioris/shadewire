//! Runs the C tests of MDL arguments (tests/mdl_args.c).

use std::ffi::CStr;
use std::os::raw::c_char;

use shadewire_capi_ctest::{shadewire_ctest_mdl_args_refused, shadewire_ctest_mdl_args_texture_branch};

fn run(test: unsafe extern "C" fn(*mut c_char, usize) -> i32) {
    let mut report = vec![0 as c_char; 512];
    let step = unsafe { test(report.as_mut_ptr(), report.len()) };
    let message = unsafe { CStr::from_ptr(report.as_ptr()) }.to_string_lossy().into_owned();
    assert_eq!(step, 0, "step {step}: {message}");
}

#[test]
fn mdl_args_texture_branch() {
    run(shadewire_ctest_mdl_args_texture_branch);
}

#[test]
fn mdl_args_refused() {
    run(shadewire_ctest_mdl_args_refused);
}
