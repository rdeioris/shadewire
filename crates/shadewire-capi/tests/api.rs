//! The C API driven through its `extern "C"` functions.

use std::ffi::{CStr, CString};
use std::ptr::{null, null_mut};

use shadewire_capi::*;

fn default_example() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../vendor/materialx/resources/Materials/Examples/StandardSurface/standard_surface_default.mtlx"
    );
    std::fs::read_to_string(path).unwrap()
}

fn to_ir(text: &str) -> *mut shadewire_ir {
    let mut ir = null_mut();
    let mut error = null_mut();
    let status =
        unsafe { shadewire_mtlx_to_ir(text.as_ptr().cast(), text.len(), null(), null(), null(), &mut ir, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    assert!(error.is_null());
    ir
}

#[test]
fn the_default_example_lowers() {
    let ir = to_ir(&default_example());
    let model = unsafe { CStr::from_ptr(shadewire_ir_model(ir)) };
    assert_eq!(model.to_str().unwrap(), "standard_surface");
    let mut len = 0usize;
    let dump = unsafe { CStr::from_ptr(shadewire_ir_dump(ir, &mut len)) };
    assert!(dump.to_str().unwrap().starts_with("model standard_surface"));
    assert_eq!(len, dump.to_bytes().len());
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn malformed_xml_is_a_parse_error_with_a_location() {
    let text = "<materialx><nodegraph></materialx>";
    let path = CString::new("bad.mtlx").unwrap();
    let mut ir = null_mut();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(text.as_ptr().cast(), text.len(), path.as_ptr(), null(), null(), &mut ir, &mut error)
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert!(ir.is_null());
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_PARSE);
    assert_eq!(unsafe { CStr::from_ptr(shadewire_error_file(error)) }.to_str().unwrap(), "bad.mtlx");
    assert!(unsafe { shadewire_error_line(error) } > 0);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn invalid_utf8_is_an_invalid_argument() {
    let bytes = [0xffu8, 0xfe];
    let mut ir = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_mtlx_to_ir(bytes.as_ptr().cast(), 2, null(), null(), null(), &mut ir, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn a_null_out_is_an_invalid_argument() {
    let text = default_example();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(text.as_ptr().cast(), text.len(), null(), null(), null(), null_mut(), &mut error)
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
}

#[test]
fn freeing_null_is_fine() {
    unsafe { shadewire_ir_free(null_mut()) };
}
