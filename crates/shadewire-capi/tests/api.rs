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

/// Without uniforms the texture takes the first binding, unless the uniform binding is reserved.
#[test]
fn reserve_uniform_binding_moves_the_textures_past_the_uniform_slot() {
    let document = r#"<materialx version="1.39">
  <image name="img" type="color3">
    <input name="file" type="filename" value="a.png" />
  </image>
  <surface_unlit name="SU" type="surfaceshader">
    <input name="emission_color" type="color3" nodename="img" />
  </surface_unlit>
  <surfacematerial name="M" type="material">
    <input name="surfaceshader" type="surfaceshader" nodename="SU" />
  </surfacematerial>
</materialx>"#;
    let mut mtlx = std::mem::MaybeUninit::<shadewire_mtlx_options>::uninit();
    unsafe { shadewire_mtlx_options_default(mtlx.as_mut_ptr()) };
    let mut mtlx = unsafe { mtlx.assume_init() };
    mtlx.values_as_uniforms = false;
    let texture_binding = |reserve: bool| {
        let mut options = std::mem::MaybeUninit::<shadewire_compile_options>::uninit();
        unsafe { shadewire_compile_options_default(options.as_mut_ptr()) };
        let mut options = unsafe { options.assume_init() };
        options.first_binding = 10;
        options.reserve_uniform_binding = reserve;
        let mut compiled = null_mut();
        let mut error = null_mut();
        let status = unsafe {
            shadewire_compile_mtlx(
                document.as_ptr().cast(),
                document.len(),
                null(),
                null(),
                &mtlx,
                &options,
                &mut compiled,
                &mut error,
            )
        };
        assert_eq!(status, shadewire_status::SHADEWIRE_OK);
        let json: serde_json::Value =
            serde_json::from_str(&text(unsafe { shadewire_compiled_reflection_json(compiled, null_mut()) })).unwrap();
        unsafe { shadewire_compiled_free(compiled) };
        assert!(json["uniform_binding"].is_null(), "{json}");
        json["textures"][0]["binding"].as_u64().unwrap()
    };
    assert_eq!((texture_binding(false), texture_binding(true)), (10, 11));
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
    let marker = sentinel::<shadewire_error>();
    let mut error: *mut shadewire_error = marker;
    let mut ir = null_mut();
    let status = unsafe {
        shadewire_mtlx_to_ir(example.as_ptr().cast(), example.len(), null(), null(), null(), &mut ir, &mut error)
    };
    assert_eq!((status, error), (shadewire_status::SHADEWIRE_OK, marker));
    let mut compiled = null_mut();
    assert_eq!(
        unsafe { shadewire_wgsl_compile(ir, null(), &mut compiled, &mut error) },
        shadewire_status::SHADEWIRE_OK
    );
    assert_eq!(error, marker);
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
    assert_eq!((status, error), (shadewire_status::SHADEWIRE_OK, marker));
    let mut blob = null_mut();
    let target = shadewire_target { kind: SHADEWIRE_TARGET_WGSL, version: 0 };
    let status = unsafe { shadewire_translate(compiled, null(), target, &mut blob, &mut error) };
    assert_eq!((status, error), (shadewire_status::SHADEWIRE_OK, marker));
    unsafe { shadewire_blob_free(blob) };
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
fn null_text_is_an_invalid_argument() {
    let mut ir = null_mut();
    let mut compiled = null_mut();
    let mut errors = [null_mut(), null_mut()];
    let statuses = unsafe {
        [
            shadewire_mtlx_to_ir(null(), 5, null(), null(), null(), &mut ir, &mut errors[0]),
            shadewire_compile_mtlx(null(), 5, null(), null(), null(), null(), &mut compiled, &mut errors[1]),
        ]
    };
    assert_eq!(statuses, [shadewire_status::SHADEWIRE_ERROR; 2]);
    assert!(ir.is_null() && compiled.is_null());
    for error in errors {
        assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
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

fn compiled_from(path_in_materials: &str) -> *mut shadewire_compiled {
    let path = format!(
        "{}/../../vendor/materialx/resources/Materials/Examples/{path_in_materials}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let cpath = CString::new(path).unwrap();
    let mut compiled = null_mut();
    let mut error = null_mut();
    let status = unsafe {
        shadewire_compile_mtlx(
            text.as_ptr().cast(),
            text.len(),
            cpath.as_ptr(),
            null(),
            null(),
            null(),
            &mut compiled,
            &mut error,
        )
    };
    assert_eq!(status, shadewire_status::SHADEWIRE_OK);
    compiled
}

fn translate_to(
    compiled: *const shadewire_compiled,
    prelude: *const std::os::raw::c_char,
    kind: shadewire_target_kind,
    version: u32,
) -> Result<*mut shadewire_blob, *mut shadewire_error> {
    let mut blob = null_mut();
    let mut error = null_mut();
    let target = shadewire_target { kind, version };
    match unsafe { shadewire_translate(compiled, prelude, target, &mut blob, &mut error) } {
        shadewire_status::SHADEWIRE_OK => Ok(blob),
        _ => Err(error),
    }
}

fn blob_bytes(blob: *const shadewire_blob) -> Vec<u8> {
    let mut len = 0usize;
    let ptr = unsafe { shadewire_blob_data(blob, &mut len) };
    unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec()
}

#[test]
fn every_target_translates_the_default_example() {
    let compiled = compiled_from("StandardSurface/standard_surface_default.mtlx");
    let cases = [
        (SHADEWIRE_TARGET_WGSL, 0),
        (SHADEWIRE_TARGET_HLSL, 50),
        (SHADEWIRE_TARGET_HLSL, 66),
        (SHADEWIRE_TARGET_GLSL, 330),
        (SHADEWIRE_TARGET_GLSL, 450),
        (SHADEWIRE_TARGET_GLSL_ES, 300),
        (SHADEWIRE_TARGET_GLSL_ES, 310),
        (SHADEWIRE_TARGET_WEBGL2, 0),
        (SHADEWIRE_TARGET_MSL, 203),
        (SHADEWIRE_TARGET_SPIRV, 0),
    ];
    for (kind, version) in cases {
        let blob = translate_to(compiled, null(), kind, version).unwrap_or_else(|_| panic!("kind {kind} {version}"));
        let bytes = blob_bytes(blob);
        assert!(!bytes.is_empty(), "kind {kind} {version}");
        let is_text = unsafe { shadewire_blob_is_text(blob) };
        assert_eq!(is_text, kind != SHADEWIRE_TARGET_SPIRV);
        if is_text {
            let ptr = unsafe { shadewire_blob_data(blob, null_mut()) };
            assert_eq!(unsafe { *ptr.add(bytes.len()) }, 0, "kind {kind} {version}: trailing NUL");
            let source = String::from_utf8(bytes).unwrap();
            match (kind, version) {
                (SHADEWIRE_TARGET_WGSL | SHADEWIRE_TARGET_HLSL, _) => {
                    assert!(source.contains("shadewire_eval_surface"), "kind {kind}")
                }
                (SHADEWIRE_TARGET_GLSL_ES, 300) => assert!(source.contains("#version 300 es")),
                _ => {}
            }
        } else {
            assert_eq!(bytes[..4], [0x03, 0x02, 0x23, 0x07]);
            assert_eq!(bytes.len() % 4, 0);
        }
        unsafe { shadewire_blob_free(blob) };
    }
    unsafe { shadewire_compiled_free(compiled) };
}

#[test]
fn a_prelude_with_a_wrong_signature_is_a_shader_error_naming_a_line() {
    let compiled = compiled_from("StandardSurface/standard_surface_brass_tiled.mtlx");
    // The reference prelude is the linked text minus the generated WGSL that follows it.
    let blob = translate_to(compiled, null(), SHADEWIRE_TARGET_WGSL, 0).unwrap();
    let linked = String::from_utf8(blob_bytes(blob)).unwrap();
    unsafe { shadewire_blob_free(blob) };
    let generated = text(unsafe { shadewire_compiled_wgsl(compiled, null_mut()) });
    let reference = linked.strip_suffix(generated.as_str()).expect("prelude comes first");
    let good = "fn shadewire_host_texcoord_0() -> vec2<f32> { return shadewire_in_texcoord_0; }";
    assert!(reference.contains(good), "{reference}");
    let prelude =
        CString::new(reference.replace(good, "fn shadewire_host_texcoord_0() -> f32 { return 0.0; }")).unwrap();
    let error = translate_to(compiled, prelude.as_ptr(), SHADEWIRE_TARGET_HLSL, 66).unwrap_err();
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_SHADER);
    let message = text(unsafe { shadewire_error_message(error, null_mut()) });
    assert!(message.contains("expected to be `vec2<f32>`, but got `f32`"), "{message}");
    let at = message.find("wgsl:").expect(&message) + "wgsl:".len();
    assert!(message[at..].starts_with(|c: char| c.is_ascii_digit()), "{message}");
    assert_eq!(unsafe { shadewire_error_line(error) }, 0, "the line is in the message, not shadewire_error_line");
    unsafe { shadewire_error_free(error) };
    unsafe { shadewire_compiled_free(compiled) };
}

#[test]
fn unknown_target_kinds_and_null_arguments_are_invalid_arguments() {
    let compiled = compiled_from("StandardSurface/standard_surface_default.mtlx");
    let marker = sentinel::<shadewire_blob>();
    let target = shadewire_target { kind: 99, version: 0 };
    let mut out = marker;
    let mut error = null_mut();
    let status = unsafe { shadewire_translate(compiled, null(), target, &mut out, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(out, marker);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
    let error = translate_to(compiled, null(), SHADEWIRE_TARGET_HLSL, 41).err().unwrap();
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
    let error = translate_to(null(), null(), SHADEWIRE_TARGET_WGSL, 0).err().unwrap();
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
    unsafe { shadewire_compiled_free(compiled) };
    unsafe { shadewire_blob_free(null_mut()) };
}

#[test]
fn an_invalid_utf8_prelude_is_an_invalid_argument() {
    let compiled = compiled_from("StandardSurface/standard_surface_default.mtlx");
    let prelude = [0xffu8, 0xfe, 0];
    let marker = sentinel::<shadewire_blob>();
    let target = shadewire_target { kind: SHADEWIRE_TARGET_HLSL, version: 60 };
    let mut out = marker;
    let mut error = null_mut();
    let status = unsafe { shadewire_translate(compiled, prelude.as_ptr().cast(), target, &mut out, &mut error) };
    assert_eq!(status, shadewire_status::SHADEWIRE_ERROR);
    assert_eq!(out, marker);
    assert_eq!(unsafe { shadewire_error_get_kind(error) }, shadewire_error_kind::SHADEWIRE_ERROR_INVALID_ARGUMENT);
    unsafe { shadewire_error_free(error) };
    unsafe { shadewire_compiled_free(compiled) };
}
