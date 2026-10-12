//! Runs the C smoke test.

use std::ffi::{CStr, CString};

use shadewire_capi_ctest::{shadewire_smoke, version};

#[test]
fn the_c_smoke_test_passes() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_default.mtlx"
    );
    let path = CString::new(path).unwrap();
    let mut report = vec![0 as std::os::raw::c_char; 512];
    let step = unsafe { shadewire_smoke(path.as_ptr(), report.as_mut_ptr(), report.len()) };
    let message = unsafe { CStr::from_ptr(report.as_ptr()) }.to_string_lossy().into_owned();
    assert_eq!(step, 0, "step {step}: {message}");
    assert_ne!(version(), 0);
}
