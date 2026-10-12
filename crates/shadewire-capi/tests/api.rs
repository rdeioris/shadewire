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

fn compile(ir: *const shadewire_ir, options: *const shadewire_compile_options) -> *mut shadewire_compiled {
    let mut compiled = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_wgsl_compile(ir, options, &mut compiled, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    compiled
}

fn text(ptr: *const std::os::raw::c_char) -> String {
    unsafe { CStr::from_ptr(ptr) }.to_str().unwrap().to_string()
}

#[test]
fn the_default_example_compiles() {
    let ir = to_ir(&default_example());
    let compiled = compile(ir, null());
    let wgsl = text(unsafe { shadewire_compiled_wgsl(compiled, null_mut()) });
    assert!(wgsl.contains("fn shadewire_eval_surface() -> StandardSurface"));
    assert_eq!(text(unsafe { shadewire_compiled_struct_name(compiled) }), "StandardSurface");
    let json: serde_json::Value =
        serde_json::from_str(&text(unsafe { shadewire_compiled_reflection_json(compiled, null_mut()) })).unwrap();
    assert_eq!(json["model"], "standard_surface");
    assert!(text(unsafe { shadewire_compiled_ir_dump(compiled, null_mut()) }).starts_with("model standard_surface"));
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn compile_mtlx_matches_the_two_step_path() {
    let example = default_example();
    let ir = to_ir(&example);
    let two_step = compile(ir, null());
    let mut one_step = null_mut();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_compile_mtlx(
            example.as_ptr().cast(),
            example.len(),
            null(),
            null(),
            null(),
            null(),
            &mut one_step,
            &mut error,
        )
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    assert_eq!(
        text(unsafe { shadewire_compiled_wgsl(one_step, null_mut()) }),
        text(unsafe { shadewire_compiled_wgsl(two_step, null_mut()) })
    );
    unsafe { shadewire_compiled_free(one_step) };
    unsafe { shadewire_compiled_free(two_step) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn compile_options_reach_the_reflection() {
    let ir = to_ir(&default_example());
    let mut options = std::mem::MaybeUninit::<shadewire_compile_options>::uninit();
    unsafe { shadewire_compile_options_default(options.as_mut_ptr()) };
    let mut options = unsafe { options.assume_init() };
    options.group = 3;
    let compiled = compile(ir, &options);
    let json: serde_json::Value =
        serde_json::from_str(&text(unsafe { shadewire_compiled_reflection_json(compiled, null_mut()) })).unwrap();
    assert_eq!(json["group"], 3);
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn compiling_a_null_ir_is_an_invalid_argument() {
    let mut compiled = null_mut();
    let mut error = null_mut();
    let status = unsafe { shadewire_wgsl_compile(null(), null(), &mut compiled, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert!(compiled.is_null());
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
    unsafe { shadewire_compiled_free(null_mut()) };
}

fn sentinel<T>() -> *mut T {
    std::ptr::NonNull::<T>::dangling().as_ptr()
}

#[test]
fn error_is_untouched_on_success() {
    let example = default_example();
    let mut error: *mut shadewire_error = null_mut();
    let mut ir = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(example.as_ptr().cast(), example.len(), null(), null(), null(), &mut ir, &mut error)
    };
    assert_eq!((status, error.is_null()), (shadewire_status::SHADEWIRE_OK, true));
    let mut compiled = null_mut();
    assert_eq!(
        unsafe { shadewire_wgsl_compile(ir, null(), &mut compiled, &mut error) },
        shadewire_status::SHADEWIRE_OK
    );
    assert!(error.is_null());
    let mut one = null_mut();
    let status = unsafe {
        shadewire_compile_mtlx(
            example.as_ptr().cast(),
            example.len(),
            null(),
            null(),
            null(),
            null(),
            &mut one,
            &mut error,
        )
    };
    assert_eq!((status, error.is_null()), (shadewire_status::SHADEWIRE_OK, true));
    unsafe { shadewire_compiled_free(one) };
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_ir_free(ir) };
}

#[test]
fn compile_mtlx_failures_leave_out_untouched() {
    let marker = sentinel::<shadewire_compiled>();
    let bad_utf8 = [0xffu8, 0xfe];
    let parse = "<materialx><nodegraph></materialx>";
    for (ptr, len) in [(bad_utf8.as_ptr().cast(), 2usize), (parse.as_ptr().cast(), parse.len())] {
        let mut out = marker;
        let mut error = null_mut();
        let status = unsafe { shadewire_compile_mtlx(ptr, len, null(), null(), null(), null(), &mut out, &mut error) };
        assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
        assert_eq!(out, marker);
        assert!(!error.is_null());
        unsafe { shadewire_error_free(error) };
    }
}

#[test]
fn getters_on_null_return_null_with_zero_len() {
    let mut len = 99usize;
    unsafe {
        assert!(shadewire_ir_model(null()).is_null());
        assert!(shadewire_ir_dump(null(), &mut len).is_null());
        assert_eq!(len, 0);
        len = 99;
        assert!(shadewire_compiled_wgsl(null(), &mut len).is_null());
        assert_eq!(len, 0);
        len = 99;
        assert!(shadewire_compiled_reflection_json(null(), &mut len).is_null());
        assert_eq!(len, 0);
        len = 99;
        assert!(shadewire_compiled_ir_dump(null(), &mut len).is_null());
        assert_eq!(len, 0);
        assert!(shadewire_compiled_struct_name(null()).is_null());
    }
}
